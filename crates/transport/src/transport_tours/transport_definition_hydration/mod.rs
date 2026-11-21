use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackRoute;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use crate::transport_tours::transport_fact_validation::transport_track_segment_is_valid;
use openzt2_zoo_state::transport_tours::tour_scoring_types::StationRatingPermille;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourViewable;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitDirection;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_circuit_types::{
    CircuitMember, CircuitMembers, OpenCircuitRequest, ReopenRestoredTransportCircuit,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::StationDestination;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::{RoutePosition, VehicleOnTrack};

#[derive(Component)]
pub(super) struct TransportDefinitionResolved;

/// Projects resolved catalogue facts onto the ordinary entities that own them.
/// Topology is deliberately not copied: construction supplies `CircuitMember`
/// and `TrackSegment` entity relations when a route is joined.
pub(super) fn hydrate_transport_entities_from_authored_world_definitions(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    unresolved_transport_entities: Query<
        (Entity, &DefinitionId),
        (
            Without<TransportDefinitionResolved>,
            Without<TransportCircuit>,
            Without<TransportTrackJunction>,
        ),
    >,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (entity, definition_identifier) in &unresolved_transport_entities {
        let station_definition = world_definitions.stations().find(|definition| {
            AssetId(definition.object.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(station_definition) = station_definition {
            commands.entity(entity).insert((
                TransportStation {
                    definition: AssetId(station_definition.id.0),
                    capacity: station_definition.capacity,
                },
                StationRatingPermille::default(),
            ));
            for [x, z, heading] in station_definition.passenger_exits.into_iter().flatten() {
                commands.spawn((
                    openzt2_zoo_state::transport_tours::transport_rider_types::TransportStationExit,
                    ChildOf(entity),
                    Transform::from_xyz(x, 0.0, z)
                        .with_rotation(Quat::from_rotation_y(heading.to_radians())),
                ));
            }
        }

        let track_definition = world_definitions.tracks().find(|definition| {
            AssetId(definition.object.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(track_definition) = track_definition {
            commands.entity(entity).insert(TrackProfile {
                definition: AssetId(track_definition.id.0),
                kind: track_definition.kind,
                maximum_grade_permille: track_definition.max_grade_permille,
            });
        }

        let vehicle_definition = world_definitions.vehicles().find(|definition| {
            AssetId(definition.object.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(vehicle_definition) = vehicle_definition {
            commands.entity(entity).insert((
                TransportVehicle {
                    definition: AssetId(vehicle_definition.id.0),
                    seats: vehicle_definition.seats,

                    maximum_speed_metres_per_second: vehicle_definition
                        .maximum_speed_metres_per_second,
                },
                LocomotionMode::Vehicle,
                Velocity::default(),
            ));
        }

        let tour_view_definition = world_definitions.tour_views().find(|definition| {
            AssetId(definition.subject.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(tour_view_definition) = tour_view_definition {
            commands.entity(entity).insert(TourViewable {
                definition: AssetId(tour_view_definition.id.0),
            });
        }
        commands.entity(entity).insert(TransportDefinitionResolved);
    }
}

/// Places newly hydrated vehicles on the first stable edge of their circuit.
///
/// Route ownership remains on `TrackSegment` entities; this stores only the
/// vehicle's current edge and scalar position. A vehicle whose circuit has no
/// complete edge remains inert instead of receiving a synthetic route.
pub(super) fn initialize_transport_vehicle_routes_from_first_stable_circuit_edge(
    mut transport_vehicles: Query<
        (Entity, &CircuitMember, &mut Transform),
        (With<TransportVehicle>, Without<RoutePosition>),
    >,
    transport_circuits: Query<(Option<&CircuitDirection>, &CircuitMembers), With<TransportCircuit>>,
    track_segments: Query<(Entity, TrackRoute, &CircuitMember)>,
    stations: Query<(), With<TransportStation>>,
    mut commands: Commands,
) {
    for (vehicle_entity, circuit_member, mut vehicle_transform) in &mut transport_vehicles {
        let Ok((direction, members)) = transport_circuits.get(circuit_member.0) else {
            continue;
        };
        let circuit_runs_in_reverse = matches!(direction, Some(CircuitDirection::Reverse));
        let Some((track_segment_entity, track_segment, _)) = track_segments
            .iter_many(members.iter())
            .filter_map(Result::ok)
            .filter(|(_, track_segment, _)| {
                transport_track_segment_is_valid(track_segment.geometry)
            })
            // A bought vehicle is created at the station, so it starts on
            // the edge leaving one; entity order alone picked any edge.
            .min_by_key(|(entity, track_segment, _)| {
                let start = if circuit_runs_in_reverse {
                    track_segment.to()
                } else {
                    track_segment.from()
                };
                (!stations.contains(start), entity.to_bits())
            })
        else {
            continue;
        };
        let initial_distance = if circuit_runs_in_reverse {
            track_segment.length
        } else {
            0.0
        };
        let initial_position = if circuit_runs_in_reverse {
            track_segment.to_position
        } else {
            track_segment.from_position
        };
        let first_destination = if circuit_runs_in_reverse {
            track_segment.from()
        } else {
            track_segment.to()
        };
        vehicle_transform.translation = initial_position;
        commands.entity(vehicle_entity).insert((
            RoutePosition {
                distance: initial_distance,
            },
            VehicleOnTrack(track_segment_entity),
            StationDestination(first_destination),
        ));
    }
}

pub(super) fn request_reopening_saved_transport_circuits(
    circuits: Query<(Entity, &CircuitMembers), With<ReopenRestoredTransportCircuit>>,
    members: Query<
        (),
        Or<(
            With<TransportStation>,
            With<TransportTrackJunction>,
            With<TrackSegment>,
            With<TransportVehicle>,
        )>,
    >,
    mut open: MessageWriter<OpenCircuitRequest>,
    mut commands: Commands,
) {
    for (circuit, membership) in &circuits {
        if membership.iter().all(|member| members.contains(member)) {
            open.write(OpenCircuitRequest {
                circuit,
                open: true,
            });
            commands
                .entity(circuit)
                .remove::<ReopenRestoredTransportCircuit>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackEnd, TrackStart};

    fn segment(
        from: Entity,
        from_position: Vec3,
        to: Entity,
        to_position: Vec3,
    ) -> (TrackStart, TrackEnd, TrackSegment) {
        let points: Box<[Vec3]> = Box::new([from_position, to_position]);
        (
            TrackStart(from),
            TrackEnd(to),
            TrackSegment {
                definition: AssetId::default(),

                from_endpoint_index: 0,
                from_position,

                to_endpoint_index: 0,
                to_position,
                path_points: points.clone(),
                travel_path_points: points,
                length: from_position.distance(to_position),
            },
        )
    }

    #[test]
    fn a_new_vehicle_starts_on_the_edge_leaving_its_station() {
        let mut app = App::new();
        app.add_systems(
            Update,
            initialize_transport_vehicle_routes_from_first_stable_circuit_edge,
        );
        let world = app.world_mut();
        let circuit = world
            .spawn(TransportCircuit {
                definition: AssetId::default(),
                closed: false,
            })
            .id();
        let junction = world.spawn_empty().id();
        let station = world
            .spawn(TransportStation {
                definition: AssetId::default(),
                capacity: 0,
            })
            .id();
        let (far, near) = (Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO);
        // Spawned first, so entity order alone would pick it.
        world.spawn((
            segment(junction, far, station, near),
            CircuitMember(circuit),
        ));
        let leaving = world
            .spawn((
                segment(station, near, junction, far),
                CircuitMember(circuit),
            ))
            .id();
        let vehicle = world
            .spawn((
                TransportVehicle {
                    definition: AssetId::default(),
                    seats: 1,

                    maximum_speed_metres_per_second: 1.0,
                },
                CircuitMember(circuit),
                Transform::default(),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<VehicleOnTrack>(vehicle)
                .map(|route| route.0),
            Some(leaving)
        );
        assert_eq!(
            app.world()
                .get::<Transform>(vehicle)
                .map(|transform| transform.translation),
            Some(near)
        );
    }
}

pub(super) fn materialize_restored_transport_prefabs(
    mut commands: Commands,
    prefabs: Res<Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
    models: Res<Assets<bevy::gltf::Gltf>>,
    server: Res<AssetServer>,
    pending: Query<(
        Entity,
        &openzt2_zoo_state::transport_tours::transport_topology_types::RestoredTransportPrefab,
        &openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
        &openzt2_world_state::world_spawn::world_membership_types::WorldMember,
        &DefinitionId,
        Option<&Transform>,
        Has<TransportVehicle>,
    )>,
) {
    for (entity, prefab, id, world, definition, pose, vehicle) in &pending {
        if matches!(
            server.get_load_state(prefab.0.id()),
            Some(bevy::asset::LoadState::Failed(_))
        ) {
            warn!(?entity, "saved transport prefab failed to load");
            commands.entity(entity).remove::<openzt2_zoo_state::transport_tours::transport_topology_types::RestoredTransportPrefab>();
            continue;
        }
        let Some(asset) = prefabs.get(&prefab.0) else {
            continue;
        };
        if openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id(asset, &models).is_some() {
            continue;
        }
        openzt2_world::world_spawn::prefab_world_instance_spawning::hydrate_loaded_scene_prefab_into_world_instance_root(
            &mut commands, entity, asset, prefab.0.clone(), world.root, definition.0, *id,
            pose.copied().unwrap_or_default(), true, None,
            if vehicle { avian3d::prelude::RigidBody::Dynamic } else { avian3d::prelude::RigidBody::Static },
        );
        commands.entity(entity).remove::<openzt2_zoo_state::transport_tours::transport_topology_types::RestoredTransportPrefab>();
    }
}
