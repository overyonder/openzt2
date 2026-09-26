use openzt2_game_data::AssetId;

use crate::plugins::{
    transport_tours::transport_circuit_types::CircuitDirection,
    world_spawn::persistent_id_types::PersistentId,
};

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::transport_snapshot_types::{
    TransportCircuitMemberSnapshotRecord, TransportCircuitSnapshotRecord, TransportSnapshotRecords,
    TransportVehicleSnapshotRecord,
};

const TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_BYTE_COUNT: usize = 28;
const TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_BYTE_COUNT: usize = 33;
const TRANSPORT_VEHICLE_SNAPSHOT_RECORD_BYTE_COUNT: usize = 33;

const TRANSPORT_CIRCUIT_SNAPSHOT_RECORD_TAG: u8 = 1;
const TRANSPORT_CIRCUIT_MEMBER_SNAPSHOT_RECORD_TAG: u8 = 2;
const TRANSPORT_VEHICLE_SNAPSHOT_RECORD_TAG: u8 = 3;

pub(super) fn append_encoded_transport_snapshot_section(
    bytes: &mut Vec<u8>,
    transport: &TransportSnapshotRecords,
) {
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
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
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
    Ok(transport)
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
