use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitDirection;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::transport_snapshot_types::{
    SkyStationCrossingSnapshot, TransportCircuitMemberSnapshotRecord,
    TransportCircuitSnapshotRecord, TransportGraphSnapshot, TransportRouteSnapshot,
    TransportSnapshotRecords, TransportVehicleSnapshotRecord,
};

const TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_BYTE_COUNT: usize = 28;
const TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_BYTE_COUNT: usize = 33;
const TRANSPORT_VEHICLE_SNAPSHOT_RECORD_BYTE_COUNT: usize = 33;

const TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_TAG: u8 = 1;
const TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_TAG: u8 = 2;
const TRANSPORT_VEHICLE_SNAPSHOT_RECORD_TAG: u8 = 3;

pub(super) fn encode_transport_snapshot_section(
    transport: &TransportSnapshotRecords,
) -> Result<Vec<u8>, WorldSnapshotPersistenceFailure> {
    let mut bytes = Vec::new();
    for circuit in &transport.circuit_records {
        bytes.push(TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_TAG);
        bytes.extend_from_slice(&circuit.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&circuit.transport_definition_identifier.0);
        bytes.push(u8::from(circuit.is_closed_loop));
        bytes.push(u8::from(circuit.is_running));
        bytes.push(match circuit.travel_direction {
            CircuitDirection::Forward => 0,
            CircuitDirection::Reverse => 1,
        });
    }
    for member in &transport.circuit_member_records {
        bytes.push(TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_TAG);
        bytes.extend_from_slice(&member.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&member.transport_definition_identifier.0);
        bytes.extend_from_slice(&member.circuit_persistent_identifier.0.to_le_bytes());
    }
    for vehicle in &transport.vehicle_records {
        bytes.push(TRANSPORT_VEHICLE_SNAPSHOT_RECORD_TAG);
        bytes.extend_from_slice(&vehicle.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&vehicle.transport_definition_identifier.0);
        bytes.extend_from_slice(&vehicle.circuit_persistent_identifier.0.to_le_bytes());
    }
    validate_graph(transport)?;
    bytes.push(2);
    bytes.extend(
        postcard::to_allocvec(&transport.graph)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
    );
    Ok(bytes)
}

pub(super) fn decode_and_validate_transport_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<TransportSnapshotRecords, WorldSnapshotPersistenceFailure> {
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    let mut cursor = 0usize;
    let mut transport = TransportSnapshotRecords::default();
    let mut last_tag = TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_TAG;
    for _ in 0..range.encoded_record_count {
        let tag = *payload
            .get(cursor)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        if tag < last_tag {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        last_tag = tag;
        let record_bytes = match tag {
            TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_TAG => TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_BYTE_COUNT,
            TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_TAG => {
                TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_BYTE_COUNT
            }
            TRANSPORT_VEHICLE_SNAPSHOT_RECORD_TAG => TRANSPORT_VEHICLE_SNAPSHOT_RECORD_BYTE_COUNT,
            _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        };
        let record = payload
            .get(cursor..cursor.saturating_add(record_bytes))
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let id =
            PersistentId(u64::from_le_bytes(record[1..9].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let mut definition = [0; 16];
        definition.copy_from_slice(&record[9..25]);
        let definition = AssetId(definition);
        if id.0 == 0 || id.0 == u64::MAX || definition.0 == [0; 16] {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        match tag {
            TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_TAG => {
                let closed = decode_snapshot_boolean_byte(record[25])?;
                let running = decode_snapshot_boolean_byte(record[26])?;
                let direction = match record[27] {
                    0 => CircuitDirection::Forward,
                    1 => CircuitDirection::Reverse,
                    _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
                };
                transport
                    .circuit_records
                    .push(TransportCircuitSnapshotRecord {
                        persistent_identifier: id,
                        transport_definition_identifier: definition,
                        is_closed_loop: closed,
                        is_running: running,
                        travel_direction: direction,
                    });
            }
            TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_TAG
            | TRANSPORT_VEHICLE_SNAPSHOT_RECORD_TAG => {
                let circuit =
                    PersistentId(u64::from_le_bytes(record[25..33].try_into().map_err(
                        |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
                    )?));
                if circuit.0 == 0 || circuit.0 == u64::MAX {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
                if tag == TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_TAG {
                    transport
                        .circuit_member_records
                        .push(TransportCircuitMemberSnapshotRecord {
                            persistent_identifier: id,
                            transport_definition_identifier: definition,
                            circuit_persistent_identifier: circuit,
                        });
                } else {
                    transport
                        .vehicle_records
                        .push(TransportVehicleSnapshotRecord {
                            persistent_identifier: id,
                            transport_definition_identifier: definition,
                            circuit_persistent_identifier: circuit,
                        });
                }
            }
            _ => unreachable!(),
        }
        cursor += record_bytes;
    }
    if cursor != payload.len() {
        transport.graph = match &payload[cursor..] {
            [1, bytes @ ..] => decode_transport_graph_v1(bytes),
            [2, bytes @ ..] => postcard::take_from_bytes(bytes)
                .ok()
                .and_then(|(graph, trailing)| trailing.is_empty().then_some(graph)),
            _ => None,
        }
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    }
    if !transport
        .circuit_records
        .windows(2)
        .all(|pair| pair[0].persistent_identifier.0 < pair[1].persistent_identifier.0)
        || !transport
            .circuit_member_records
            .windows(2)
            .all(|pair| pair[0].persistent_identifier.0 < pair[1].persistent_identifier.0)
        || !transport
            .vehicle_records
            .windows(2)
            .all(|pair| pair[0].persistent_identifier.0 < pair[1].persistent_identifier.0)
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut all_ids = transport
        .circuit_records
        .iter()
        .map(|row| row.persistent_identifier)
        .chain(
            transport
                .circuit_member_records
                .iter()
                .map(|row| row.persistent_identifier),
        )
        .chain(
            transport
                .vehicle_records
                .iter()
                .map(|row| row.persistent_identifier),
        )
        .collect::<Vec<_>>();
    all_ids.sort_unstable_by_key(|id| id.0);
    if all_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
    }
    let circuit_exists = |id: PersistentId| {
        transport
            .circuit_records
            .binary_search_by_key(&id.0, |row| row.persistent_identifier.0)
            .is_ok()
    };
    if transport
        .circuit_member_records
        .iter()
        .any(|row| !circuit_exists(row.circuit_persistent_identifier))
        || transport
            .vehicle_records
            .iter()
            .any(|row| !circuit_exists(row.circuit_persistent_identifier))
    {
        return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
    }
    validate_graph(&transport)?;
    Ok(transport)
}

fn decode_transport_graph_v1(bytes: &[u8]) -> Option<TransportGraphSnapshot> {
    type GraphPrefix = (Vec<(u64, bevy::prelude::Transform)>, Vec<(u64, i64)>);
    type GraphSuffix = (Vec<(u64, Option<f32>)>, Vec<TransportRouteSnapshot>);
    let ((poses, fares), bytes) = postcard::take_from_bytes::<GraphPrefix>(bytes).ok()?;
    // Version 1 omitted either of these vectors entirely when empty.
    for (has_fare_dollars, has_crossings) in
        [(true, true), (true, false), (false, true), (false, false)]
    {
        let Some((fares_dollars, after_fares)) = (if has_fare_dollars {
            postcard::take_from_bytes::<Vec<(u64, f32)>>(bytes)
                .ok()
                .filter(|(fares, _)| !fares.is_empty())
        } else {
            Some((Vec::new(), bytes))
        }) else {
            continue;
        };
        let Some((sky_station_crossings, after_crossings)) = (if has_crossings {
            postcard::take_from_bytes::<Vec<SkyStationCrossingSnapshot>>(after_fares)
                .ok()
                .filter(|(crossings, _)| !crossings.is_empty())
        } else {
            Some((Vec::new(), after_fares))
        }) else {
            continue;
        };
        if let Ok(((junctions, routes), [])) =
            postcard::take_from_bytes::<GraphSuffix>(after_crossings)
        {
            return Some(TransportGraphSnapshot {
                poses,
                fares,
                fares_dollars,
                sky_station_crossings,
                junctions,
                routes,
                station_ratings: Vec::new(),
            });
        }
    }
    None
}

fn decode_snapshot_boolean_byte(
    encoded_boolean_byte: u8,
) -> Result<bool, WorldSnapshotPersistenceFailure> {
    match encoded_boolean_byte {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
    }
}

fn validate_graph(
    records: &TransportSnapshotRecords,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    use WorldSnapshotPersistenceFailure::{
        BrokenPersistentEntityReference, CorruptSnapshotSection,
    };
    let graph = &records.graph;
    let member = |id| {
        records
            .circuit_member_records
            .binary_search_by_key(&id, |row| row.persistent_identifier.0)
            .ok()
            .map(|index| records.circuit_member_records[index].circuit_persistent_identifier)
    };
    let known_vehicle = |id| {
        records
            .vehicle_records
            .binary_search_by_key(&id, |row| row.persistent_identifier.0)
            .is_ok()
    };
    let known_circuit = |id| {
        records
            .circuit_records
            .binary_search_by_key(&id, |row| row.persistent_identifier.0)
            .is_ok()
    };
    if !graph
        .fares_dollars
        .windows(2)
        .all(|rows| rows[0].0 < rows[1].0)
        || graph.fares_dollars.iter().any(|(id, fare)| {
            !known_circuit(*id)
                || !fare.is_finite()
                || *fare < 0.0
                || graph
                    .fares
                    .binary_search_by_key(id, |(saved, _)| *saved)
                    .is_ok()
        })
        || !graph
            .sky_station_crossings
            .windows(2)
            .all(|rows| rows[0].vehicle < rows[1].vehicle)
        || graph.sky_station_crossings.iter().any(|crossing| {
            !known_vehicle(crossing.vehicle)
                || !crossing.distance.is_finite()
                || crossing.distance < 0.0
                || !graph
                    .routes
                    .iter()
                    .any(|route| route.pieces.first() == Some(&crossing.next_route))
        })
        || !graph
            .station_ratings
            .windows(2)
            .all(|rows| rows[0].station < rows[1].station)
        || graph.station_ratings.iter().any(|rating| {
            member(rating.station).is_none()
                || rating.permille > 1000
                || rating.samples.len() > usize::from(u16::MAX)
                || rating.samples.iter().any(|sample| !sample.is_finite())
        })
        || !graph.poses.windows(2).all(|rows| rows[0].0 < rows[1].0)
        || !graph.fares.windows(2).all(|rows| rows[0].0 < rows[1].0)
        || !graph.junctions.windows(2).all(|rows| rows[0].0 < rows[1].0)
        || graph.poses.iter().any(|(id, pose)| {
            member(*id).is_none() && !known_vehicle(*id)
                || !pose.translation.is_finite()
                || !pose.scale.is_finite()
                || pose.scale.min_element() <= 0.0
                || !pose.rotation.is_finite()
                || (pose.rotation.length_squared() - 1.0).abs() > 0.001
        })
        || graph
            .fares
            .iter()
            .any(|(id, fare)| !known_circuit(*id) || *fare < 0)
        || graph.junctions.iter().any(|(id, height)| {
            member(*id).is_none()
                || height.is_some_and(|height| !height.is_finite() || height <= 0.0)
        })
    {
        return Err(CorruptSnapshotSection);
    }
    let mut used_pieces = std::collections::BTreeSet::new();
    for route in &graph.routes {
        let Some(&id) = route.pieces.first() else {
            return Err(CorruptSnapshotSection);
        };
        let Some(circuit) = member(id) else {
            return Err(BrokenPersistentEntityReference);
        };
        if member(route.from) != Some(circuit) || member(route.to) != Some(circuit) {
            return Err(BrokenPersistentEntityReference);
        }
        if route.from == route.to
            || route.from_socket >= 2
            || route.to_socket >= 2
            || route.definition == AssetId::default()
            || route.pieces.len() > usize::from(u16::MAX)
            || route.points.len() != route.pieces.len() + 1
            || route.travel_points.len() < 2
            || route
                .points
                .iter()
                .chain(&route.travel_points)
                .any(|point| !point.is_finite())
            || route.points.first() != route.travel_points.first()
            || route.points.last() != route.travel_points.last()
            || !route.length.is_finite()
            || route.length <= 0.0
            || route.pieces.iter().enumerate().any(|(index, &piece)| {
                matches!(piece, 0 | u64::MAX)
                    || !used_pieces.insert(piece)
                    || index != 0
                        && (member(piece).is_some() || known_vehicle(piece) || known_circuit(piece))
            })
        {
            return Err(CorruptSnapshotSection);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::transport_snapshot_types::StationTourRatingSnapshot;
    use super::*;

    #[test]
    fn station_rating_keeps_the_ordered_samples_and_published_value() {
        let mut records = TransportSnapshotRecords::default();
        records
            .circuit_records
            .push(TransportCircuitSnapshotRecord {
                persistent_identifier: PersistentId(1),
                transport_definition_identifier: AssetId::from_key("circuit"),
                is_closed_loop: true,
                is_running: true,
                travel_direction: CircuitDirection::Forward,
            });
        records
            .circuit_member_records
            .push(TransportCircuitMemberSnapshotRecord {
                persistent_identifier: PersistentId(2),
                circuit_persistent_identifier: PersistentId(1),
                transport_definition_identifier: AssetId::from_key("station"),
            });
        records
            .graph
            .station_ratings
            .push(StationTourRatingSnapshot {
                station: 2,
                permille: 713,
                samples: vec![0.75, -0.25, 0.5],
            });
        let bytes = encode_transport_snapshot_section(&records).unwrap();
        let restored = decode_and_validate_transport_snapshot_section(
            &bytes,
            WorldSnapshotSectionDirectoryEntry {
                encoded_record_count: 2,
                payload_start_byte_offset: 0,
                payload_byte_count: bytes.len() as u64,
            },
        )
        .unwrap();
        let [rating] = restored.graph.station_ratings.as_slice() else {
            panic!("expected one station rating");
        };
        assert_eq!(rating.station, 2);
        assert_eq!(rating.permille, 713);
        assert_eq!(rating.samples, [0.75, -0.25, 0.5]);
    }
}
