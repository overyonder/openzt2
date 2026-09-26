use bevy::prelude::{Commands, Entity, Or, Query, With};

use crate::plugins::{
    transport_tours::{
        transport_circuit_types::{CircuitMember, CircuitRunning, TransportCircuit},
        transport_vehicle_types::TransportVehicle,
    },
    world_spawn::{
        persistent_id_types::PersistentId, persistent_id_types::PersistentIdAllocator,
        world_membership_types::DefinitionId, world_membership_types::WorldMember,
    },
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    transport_snapshot_types::TransportSnapshotRecords,
};

pub(super) fn apply_transport_snapshot_records_to_live_transport_entities(
    commands: &mut Commands,
    transport_snapshot_records: TransportSnapshotRecords,
    world_root_entity: Entity,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
    transport_entities_with_persistent_identifiers: &Query<
        (Entity, &PersistentId),
        Or<(
            With<TransportCircuit>,
            With<CircuitMember>,
            With<TransportVehicle>,
        )>,
    >,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let mut saved_transport_persistent_identifiers = transport_snapshot_records
        .circuit_records
        .iter()
        .map(|record| record.persistent_identifier)
        .chain(
            transport_snapshot_records
                .circuit_member_records
                .iter()
                .map(|record| record.persistent_identifier),
        )
        .chain(
            transport_snapshot_records
                .vehicle_records
                .iter()
                .map(|record| record.persistent_identifier),
        )
        .collect::<Vec<_>>();
    saved_transport_persistent_identifiers
        .sort_unstable_by_key(|persistent_identifier| persistent_identifier.0);

    let mut stale_transport_persistent_identifiers = Vec::new();
    for (transport_entity, persistent_identifier) in transport_entities_with_persistent_identifiers
    {
        if saved_transport_persistent_identifiers
            .binary_search_by_key(&persistent_identifier.0, |candidate| candidate.0)
            .is_err()
        {
            stale_transport_persistent_identifiers.push(*persistent_identifier);
            commands.entity(transport_entity).despawn();
        }
    }
    stale_transport_persistent_identifiers
        .sort_unstable_by_key(|persistent_identifier| persistent_identifier.0);

    let mut restored_entities_by_persistent_identifier = entities_with_persistent_identifiers
        .iter()
        .filter(|(_, persistent_identifier)| {
            stale_transport_persistent_identifiers
                .binary_search_by_key(&persistent_identifier.0, |candidate| candidate.0)
                .is_err()
        })
        .map(|(entity, persistent_identifier)| (*persistent_identifier, entity))
        .collect::<Vec<_>>();
    restored_entities_by_persistent_identifier
        .sort_unstable_by_key(|(persistent_identifier, _)| persistent_identifier.0);

    let imported_transport_persistent_identifiers = transport_snapshot_records
        .circuit_records
        .iter()
        .map(|record| record.persistent_identifier)
        .chain(
            transport_snapshot_records
                .circuit_member_records
                .iter()
                .map(|record| record.persistent_identifier),
        )
        .chain(
            transport_snapshot_records
                .vehicle_records
                .iter()
                .map(|record| record.persistent_identifier),
        )
        .filter(|persistent_identifier| {
            restored_entities_by_persistent_identifier
                .binary_search_by_key(&persistent_identifier.0, |(candidate, _)| candidate.0)
                .is_err()
        });
    for imported_transport_persistent_identifier in imported_transport_persistent_identifiers {
        persistent_identifier_allocator
            .reserve_imported(world_root_entity, imported_transport_persistent_identifier)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    }

    for transport_circuit_snapshot_record in transport_snapshot_records.circuit_records {
        let transport_circuit_entity = restored_entities_by_persistent_identifier
            .binary_search_by_key(
                &transport_circuit_snapshot_record.persistent_identifier.0,
                |(persistent_identifier, _)| persistent_identifier.0,
            )
            .ok()
            .map(|index| restored_entities_by_persistent_identifier[index].1)
            .unwrap_or_else(|| {
                let entity = commands
                    .spawn((
                        transport_circuit_snapshot_record.persistent_identifier,
                        DefinitionId(
                            transport_circuit_snapshot_record.transport_definition_identifier,
                        ),
                        WorldMember {
                            root: world_root_entity,
                        },
                    ))
                    .id();
                restored_entities_by_persistent_identifier.push((
                    transport_circuit_snapshot_record.persistent_identifier,
                    entity,
                ));
                restored_entities_by_persistent_identifier
                    .sort_unstable_by_key(|(persistent_identifier, _)| persistent_identifier.0);
                entity
            });
        commands.entity(transport_circuit_entity).insert((
            DefinitionId(transport_circuit_snapshot_record.transport_definition_identifier),
            TransportCircuit {
                definition: transport_circuit_snapshot_record.transport_definition_identifier,
                closed: transport_circuit_snapshot_record.is_closed_loop,
            },
            CircuitRunning(transport_circuit_snapshot_record.is_running),
            transport_circuit_snapshot_record.travel_direction,
        ));
    }

    for circuit_member_snapshot_record in transport_snapshot_records.circuit_member_records {
        let transport_circuit_entity = restored_entities_by_persistent_identifier
            .binary_search_by_key(
                &circuit_member_snapshot_record
                    .circuit_persistent_identifier
                    .0,
                |(persistent_identifier, _)| persistent_identifier.0,
            )
            .ok()
            .map(|index| restored_entities_by_persistent_identifier[index].1)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let circuit_member_entity = restored_entities_by_persistent_identifier
            .binary_search_by_key(
                &circuit_member_snapshot_record.persistent_identifier.0,
                |(persistent_identifier, _)| persistent_identifier.0,
            )
            .ok()
            .map(|index| restored_entities_by_persistent_identifier[index].1)
            .unwrap_or_else(|| {
                let entity = commands
                    .spawn((
                        circuit_member_snapshot_record.persistent_identifier,
                        DefinitionId(
                            circuit_member_snapshot_record.transport_definition_identifier,
                        ),
                        WorldMember {
                            root: world_root_entity,
                        },
                    ))
                    .id();
                restored_entities_by_persistent_identifier
                    .push((circuit_member_snapshot_record.persistent_identifier, entity));
                restored_entities_by_persistent_identifier
                    .sort_unstable_by_key(|(persistent_identifier, _)| persistent_identifier.0);
                entity
            });
        commands.entity(circuit_member_entity).insert((
            DefinitionId(circuit_member_snapshot_record.transport_definition_identifier),
            CircuitMember(transport_circuit_entity),
        ));
    }

    for transport_vehicle_snapshot_record in transport_snapshot_records.vehicle_records {
        let transport_circuit_entity = restored_entities_by_persistent_identifier
            .binary_search_by_key(
                &transport_vehicle_snapshot_record
                    .circuit_persistent_identifier
                    .0,
                |(persistent_identifier, _)| persistent_identifier.0,
            )
            .ok()
            .map(|index| restored_entities_by_persistent_identifier[index].1)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let transport_vehicle_entity = restored_entities_by_persistent_identifier
            .binary_search_by_key(
                &transport_vehicle_snapshot_record.persistent_identifier.0,
                |(persistent_identifier, _)| persistent_identifier.0,
            )
            .ok()
            .map(|index| restored_entities_by_persistent_identifier[index].1)
            .unwrap_or_else(|| {
                let entity = commands
                    .spawn((
                        transport_vehicle_snapshot_record.persistent_identifier,
                        DefinitionId(
                            transport_vehicle_snapshot_record.transport_definition_identifier,
                        ),
                        WorldMember {
                            root: world_root_entity,
                        },
                    ))
                    .id();
                restored_entities_by_persistent_identifier.push((
                    transport_vehicle_snapshot_record.persistent_identifier,
                    entity,
                ));
                restored_entities_by_persistent_identifier
                    .sort_unstable_by_key(|(persistent_identifier, _)| persistent_identifier.0);
                entity
            });
        commands.entity(transport_vehicle_entity).insert((
            DefinitionId(transport_vehicle_snapshot_record.transport_definition_identifier),
            CircuitMember(transport_circuit_entity),
        ));
    }

    Ok(())
}
