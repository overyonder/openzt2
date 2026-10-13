use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::{
    transport_snapshot_types::{
        TransportCircuitMemberSnapshotRecord, TransportCircuitSnapshotRecord,
        TransportSnapshotRecords, TransportVehicleSnapshotRecord,
    },
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

pub(super) fn capture_sorted_transport_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
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
        _circuit_entity,
        persistent_identifier,
        transport_circuit,
        optional_running_state,
        optional_travel_direction,
    ) in &world_snapshot_capture_queries.transport_circuits_with_snapshot_state
    {
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
    for (persistent_identifier, circuit_member, definition_identifier) in
        &world_snapshot_capture_queries.transport_circuit_members_with_definition_identifiers
    {
        let Some(circuit_persistent_identifier) = entity_persistent_identifier(circuit_member.0)
        else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
        transport_snapshot_records.circuit_member_records.push(
            TransportCircuitMemberSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                transport_definition_identifier: definition_identifier.0,
                circuit_persistent_identifier,
            },
        );
    }
    for (persistent_identifier, circuit_member, definition_identifier) in
        &world_snapshot_capture_queries.transport_vehicles_with_definition_identifiers
    {
        let Some(circuit_persistent_identifier) = entity_persistent_identifier(circuit_member.0)
        else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
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
    Ok(transport_snapshot_records)
}
