use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::{
    economy::{facility_economy_types::Price, money_types::Money},
    transport_tours::{
        transport_topology_types::{RestoredTransportPrefab, SkyTowerHeight, TrackEnd, TrackProfile, TrackSegment, TrackStart, TransportTrackJunction},
        transport_track_construction_types::TransportTrackConstructionPiece,
    },
};
use bevy::prelude::*;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_circuit_types::{
    CircuitRunning, ReopenRestoredTransportCircuit,
};
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    transport_snapshot_types::TransportSnapshotRecords,
};

pub(super) fn apply_transport_snapshot_records_to_live_transport_entities(
    commands: &mut Commands,
    transport_snapshot_records: TransportSnapshotRecords,
    definitions: WorldDefinitionsView<'_>,
    restored_placements: &std::collections::BTreeMap<u64, Entity>,
    world_root_entity: Entity,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
    transport_entities_with_persistent_identifiers: &Query<
        (Entity, &PersistentId),
        Or<(
            With<TransportCircuit>,
            With<CircuitMember>,
            With<TransportVehicle>,
            With<TransportTrackConstructionPiece>,
        )>,
    >,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let graph = transport_snapshot_records.graph;
    let additional_pieces = || {
        graph
            .routes
            .iter()
            .flat_map(|route| route.pieces.iter().skip(1))
            .map(|&id| PersistentId(id))
    };
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
        .chain(additional_pieces())
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
    restored_entities_by_persistent_identifier.extend(
        restored_placements
            .iter()
            .map(|(&id, &entity)| (PersistentId(id), entity)),
    );
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
        .chain(additional_pieces())
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

    let lookup_pose = |id: PersistentId| {
        graph
            .poses
            .binary_search_by_key(&id.0, |(id, _)| *id)
            .ok()
            .map(|index| graph.poses[index].1)
    };
    let find_model = |definition| {
        definitions
            .find_object(definition)
            .and_then(|object| definitions.scene(object.prefab))
    };

    let has_saved_routes = !graph.routes.is_empty();
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
        let mut circuit = commands.entity(transport_circuit_entity);
        if has_saved_routes && !transport_circuit_snapshot_record.is_closed_loop {
            circuit.insert(ReopenRestoredTransportCircuit);
        } else {
            circuit.remove::<ReopenRestoredTransportCircuit>();
        }
        circuit.insert((
            DefinitionId(transport_circuit_snapshot_record.transport_definition_identifier),
            TransportCircuit {
                definition: transport_circuit_snapshot_record.transport_definition_identifier,
                closed: has_saved_routes || transport_circuit_snapshot_record.is_closed_loop,
            },
            CircuitRunning(transport_circuit_snapshot_record.is_running),
            Price(Money(
                graph
                    .fares
                    .binary_search_by_key(
                        &transport_circuit_snapshot_record.persistent_identifier.0,
                        |(id, _)| *id,
                    )
                    .ok()
                    .map_or(0, |index| graph.fares[index].1),
            )),
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
                let object = circuit_member_snapshot_record.transport_definition_identifier;
                if graph
                    .junctions
                    .binary_search_by_key(
                        &circuit_member_snapshot_record.persistent_identifier.0,
                        |(id, _)| *id,
                    )
                    .is_err()
                {
                    if let Some(model) = definitions
                        .stations()
                        .find(|station| station.id == object || station.object == object)
                        .and_then(|station| find_model(station.object))
                    {
                        commands
                            .entity(entity)
                            .insert(RestoredTransportPrefab(model));
                    }
                }

                restored_entities_by_persistent_identifier
                    .push((circuit_member_snapshot_record.persistent_identifier, entity));
                restored_entities_by_persistent_identifier
                    .sort_unstable_by_key(|(persistent_identifier, _)| persistent_identifier.0);
                entity
            });

        if let Some(pose) = lookup_pose(circuit_member_snapshot_record.persistent_identifier) {
            commands
                .entity(circuit_member_entity)
                .insert((pose, Visibility::Inherited));
        }
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
                let object = transport_vehicle_snapshot_record.transport_definition_identifier;
                if let Some(model) = definitions
                    .vehicles()
                    .find(|vehicle| vehicle.id == object || vehicle.object == object)
                    .and_then(|vehicle| find_model(vehicle.object))
                {
                    commands
                        .entity(entity)
                        .insert(RestoredTransportPrefab(model));
                }

                restored_entities_by_persistent_identifier.push((
                    transport_vehicle_snapshot_record.persistent_identifier,
                    entity,
                ));
                restored_entities_by_persistent_identifier
                    .sort_unstable_by_key(|(persistent_identifier, _)| persistent_identifier.0);
                entity
            });

        if let Some(pose) = lookup_pose(transport_vehicle_snapshot_record.persistent_identifier) {
            commands
                .entity(transport_vehicle_entity)
                .insert((pose, Visibility::Inherited));
        }
        commands.entity(transport_vehicle_entity).insert((
            DefinitionId(transport_vehicle_snapshot_record.transport_definition_identifier),
            CircuitMember(transport_circuit_entity),
        ));
    }

    let mut entities: std::collections::BTreeMap<u64, Entity> =
        restored_entities_by_persistent_identifier
            .into_iter()
            .map(|(id, entity)| (id.0, entity))
            .collect();
    for (id, tower) in graph.junctions {
        let entity = entities[&id];
        let mut junction = commands.entity(entity);
        junction.insert(TransportTrackJunction);
        if let Some(height) = tower {
            junction.insert(SkyTowerHeight(height));
        }
    }
    for saved in graph.routes {
        let (Some(&first), Some(&from), Some(&to)) = (
            entities.get(&saved.pieces[0]),
            entities.get(&saved.from),
            entities.get(&saved.to),
        ) else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
        let profile = TrackProfile {
            definition: saved.definition,
            kind: saved.kind,
            maximum_grade_permille: saved.maximum_grade_permille,
        };
        for (index, (&id, points)) in saved.pieces.iter().zip(saved.points.windows(2)).enumerate() {
            let entity = *entities.entry(id).or_insert_with(|| {
                commands
                    .spawn((
                        PersistentId(id),
                        DefinitionId(saved.definition),
                        WorldMember {
                            root: world_root_entity,
                        },
                    ))
                    .id()
            });
            let direction = points[1] - points[0];
            let transform = Transform::from_translation(points[0].midpoint(points[1]))
                .with_rotation(Quat::from_rotation_y(direction.x.atan2(direction.z)));
            commands.entity(entity).insert((
                profile,
                transform,
                Visibility::Inherited,
                TransportTrackConstructionPiece {
                    route_segment: first,
                    piece_index: index as u16,
                },
            ));
        }
        commands.entity(first).insert((
            TrackStart(from),
            TrackEnd(to),
            TrackSegment {
                definition: saved.definition,
                from_endpoint_index: saved.from_socket,
                from_position: saved.points[0],
                to_endpoint_index: saved.to_socket,
                to_position: *saved.points.last().expect("validated track"),
                path_points: saved.points.into_boxed_slice(),
                travel_path_points: saved.travel_points.into_boxed_slice(),
                length: saved.length,
            },
        ));
    }
    Ok(())
}
