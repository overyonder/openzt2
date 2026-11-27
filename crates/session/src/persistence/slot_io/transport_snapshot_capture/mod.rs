use bevy::ecs::relationship::RelationshipTarget;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::{
    transport_snapshot_types::{
        StationTourRatingSnapshot, TransportCircuitMemberSnapshotRecord,
        TransportCircuitSnapshotRecord, TransportRouteSnapshot, TransportSnapshotRecords,
        TransportVehicleSnapshotRecord,
    },
    world_snapshot_capture_system_parameters::TransportCaptureQueries,
};

pub(super) fn capture_sorted_transport_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &TransportCaptureQueries,
) -> Result<TransportSnapshotRecords, WorldSnapshotPersistenceFailure> {
    let entity_persistent_identifier = |entity| {
        world_snapshot_capture_queries
            .entities_with_persistent_identifiers
            .get(entity)
            .ok()
            .copied()
    };
    let mut transport_snapshot_records = TransportSnapshotRecords::default();

    for (
        circuit_entity,
        persistent_identifier,
        transport_circuit,
        optional_running_state,
        optional_travel_direction,
    ) in &world_snapshot_capture_queries.transport_circuits_with_snapshot_state
    {
        if let Ok(price) = world_snapshot_capture_queries
            .transport_fares
            .get(circuit_entity)
        {
            transport_snapshot_records
                .graph
                .fares_dollars
                .push((persistent_identifier.0, price.0.dollars()));
        }
        transport_snapshot_records
            .circuit_records
            .push(TransportCircuitSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                transport_definition_identifier: transport_circuit.definition,
                is_closed_loop: transport_circuit.closed,
                is_running: optional_running_state.is_none_or(|running_state| running_state.0),
                travel_direction: optional_travel_direction.copied().unwrap_or_default(),
            });
    }
    for (entity, persistent_identifier, circuit_member, definition_identifier) in
        &world_snapshot_capture_queries.transport_circuit_members_with_definition_identifiers
    {
        let Some(circuit_persistent_identifier) = entity_persistent_identifier(circuit_member.0)
        else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
        if let Ok(pose) = world_snapshot_capture_queries
            .transport_entity_poses
            .get(entity)
        {
            transport_snapshot_records
                .graph
                .poses
                .push((persistent_identifier.0, *pose));
        }
        if let Ok((rating, history)) = world_snapshot_capture_queries
            .station_tour_ratings
            .get(entity)
        {
            if rating.0 != 0 || !history.0.is_empty() {
                transport_snapshot_records
                    .graph
                    .station_ratings
                    .push(StationTourRatingSnapshot {
                        station: persistent_identifier.0,
                        permille: rating.0,
                        samples: history.0.iter().copied().collect(),
                    });
            }
        }
        transport_snapshot_records.circuit_member_records.push(
            TransportCircuitMemberSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                transport_definition_identifier: definition_identifier.0,
                circuit_persistent_identifier,
            },
        );
    }
    for (entity, persistent_identifier, circuit_member, definition_identifier) in
        &world_snapshot_capture_queries.transport_vehicles_with_definition_identifiers
    {
        let Some(circuit_persistent_identifier) = entity_persistent_identifier(circuit_member.0)
        else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
        if let Ok(pose) = world_snapshot_capture_queries
            .transport_entity_poses
            .get(entity)
        {
            transport_snapshot_records
                .graph
                .poses
                .push((persistent_identifier.0, *pose));
        }
        if let Ok(crossing) = world_snapshot_capture_queries
            .sky_station_crossings
            .get(entity)
        {
            let next_route = entity_persistent_identifier(crossing.next_track)
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            transport_snapshot_records.graph.sky_station_crossings.push(
                super::transport_snapshot_types::SkyStationCrossingSnapshot {
                    vehicle: persistent_identifier.0,
                    next_route: next_route.0,
                    reverse: crossing.reverse,
                    distance: crossing.distance,
                },
            );
        }
        transport_snapshot_records
            .vehicle_records
            .push(TransportVehicleSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                transport_definition_identifier: definition_identifier.0,
                circuit_persistent_identifier,
            });
    }

    transport_snapshot_records
        .circuit_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    transport_snapshot_records
        .circuit_member_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    transport_snapshot_records
        .vehicle_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    let graph = &mut transport_snapshot_records.graph;
    for (id, tower) in &world_snapshot_capture_queries.transport_junctions {
        graph.junctions.push((id.0, tower.map(|tower| tower.0)));
    }
    for (id, route, profile, pieces) in &world_snapshot_capture_queries.transport_track_routes {
        let mut saved_pieces = world_snapshot_capture_queries
            .transport_track_pieces
            .iter_many(pieces.iter())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        saved_pieces.sort_unstable_by_key(|(_, piece)| piece.piece_index);
        if saved_pieces.first().is_none_or(|(first, _)| *first != id)
            || saved_pieces.len() + 1 != route.path_points.len()
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        graph.routes.push(TransportRouteSnapshot {
            pieces: saved_pieces.into_iter().map(|(id, _)| id.0).collect(),
            definition: route.definition,
            kind: profile.kind,
            maximum_grade_permille: profile.maximum_grade_permille,
            from: entity_persistent_identifier(route.from())
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?
                .0,
            from_socket: route.from_endpoint_index,
            to: entity_persistent_identifier(route.to())
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?
                .0,
            to_socket: route.to_endpoint_index,
            points: route.path_points.to_vec(),
            travel_points: route.travel_path_points.to_vec(),
            length: route.length,
        });
    }
    graph.fares_dollars.sort_unstable_by_key(|(id, _)| *id);
    graph
        .station_ratings
        .sort_unstable_by_key(|rating| rating.station);
    graph
        .sky_station_crossings
        .sort_unstable_by_key(|crossing| crossing.vehicle);
    graph.poses.sort_unstable_by_key(|(id, _)| *id);
    graph.junctions.sort_unstable_by_key(|(id, _)| *id);
    graph.routes.sort_unstable_by_key(|route| route.pieces[0]);
    Ok(transport_snapshot_records)
}
