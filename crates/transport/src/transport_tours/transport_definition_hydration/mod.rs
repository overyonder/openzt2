use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::transportation_and_tours::TransportationTrackKind, AssetId,
};
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportStationExit;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackRoute;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use crate::transport_tours::transport_route_position_calculations::TrackTravelDistances;
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

#[derive(Resource, Default, PartialEq)]
pub(super) struct TourViewCatalogue {
    by_subject: bevy::platform::collections::HashMap<AssetId, AssetId>,
    pub(super) maximum_radius_m: f32,
}

pub(super) fn index_authored_tour_views(
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut catalogue: ResMut<TourViewCatalogue>,
) {
    if !assets.is_changed() && !active.is_changed() {
        return;
    }
    let mut next = TourViewCatalogue::default();
    if let Some(definitions) = active.get(&assets) {
        for view in definitions.tour_views() {
            let id = AssetId(view.id.0);
            next.by_subject.entry(AssetId(view.subject.0)).or_insert(id);
            next.by_subject.entry(id).or_insert(id);
            next.maximum_radius_m = next.maximum_radius_m.max(view.radius_cm as f32 * 0.01);
        }
    }
    catalogue.set_if_neq(next);
}

pub(super) fn hydrate_transport_entities_from_authored_world_definitions(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    views: Res<TourViewCatalogue>,
    changed: Query<
        Entity,
        (
            Changed<DefinitionId>,
            Without<TransportCircuit>,
            Without<TransportTrackJunction>,
        ),
    >,
    mut entities: Query<
        (
            Entity,
            &DefinitionId,
            Option<&Children>,
            Option<&mut TransportStation>,
            Has<StationRatingPermille>,
            Option<&mut TrackProfile>,
            Option<&mut TransportVehicle>,
            Option<&mut LocomotionMode>,
            Has<Velocity>,
            Option<&mut TourViewable>,
        ),
        (Without<TransportCircuit>, Without<TransportTrackJunction>),
    >,
    mut exits: Query<&mut Transform, With<TransportStationExit>>,
    mut commands: Commands,
) {
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let catalogue_changed =
        active_world_definitions.is_changed() || world_definition_assets.is_changed();
    let mut updated = bevy::ecs::entity::EntityHashSet::default();
    if catalogue_changed {
        updated.extend(entities.iter().map(|(entity, ..)| entity));
    }
    updated.extend(changed.iter());
    for entity in updated {
        let Ok((
            _,
            source,
            children,
            station,
            rating,
            track,
            vehicle,
            mode,
            has_velocity,
            viewable,
        )) = entities.get_mut(entity)
        else {
            continue;
        };
        if let Some(definition) = definitions.find_station(source.0) {
            let value = TransportStation {
                definition: AssetId(definition.id.0),
                capacity: definition.capacity,
            };
            if let Some(mut station) = station {
                station.set_if_neq(value);
            } else {
                commands.entity(entity).insert(value);
            }
            if !rating {
                commands
                    .entity(entity)
                    .insert(StationRatingPermille::default());
            }
            let mut existing = children
                .into_iter()
                .flat_map(|children| children.iter())
                .filter(|child| exits.contains(*child))
                .collect::<Vec<_>>()
                .into_iter();
            for [x, z, heading] in definition.passenger_exits.into_iter().flatten() {
                let pose = Transform::from_xyz(x, 0.0, z)
                    .with_rotation(Quat::from_rotation_y(heading.to_radians()));
                if let Some(exit) = existing.next() {
                    if let Ok(mut transform) = exits.get_mut(exit) {
                        transform.set_if_neq(pose);
                    }
                } else {
                    commands.spawn((TransportStationExit, ChildOf(entity), pose));
                }
            }
            for unused in existing {
                commands.entity(unused).try_despawn();
            }
        }
        if let Some(definition) = definitions.find_track(source.0) {
            let value = TrackProfile {
                definition: AssetId(definition.id.0),
                kind: definition.kind,
                maximum_grade_permille: definition.max_grade_permille,
            };
            if let Some(mut track) = track {
                track.set_if_neq(value);
            } else {
                commands.entity(entity).insert(value);
            }
        }
        if let Some(definition) = definitions.find_vehicle(source.0) {
            let value = TransportVehicle {
                definition: AssetId(definition.id.0),
                seats: definition.seats,
                maximum_speed_metres_per_second: definition.maximum_speed_metres_per_second,
            };
            if let Some(mut vehicle) = vehicle {
                vehicle.set_if_neq(value);
            } else {
                commands.entity(entity).insert(value);
            }
            if let Some(mut mode) = mode {
                mode.set_if_neq(LocomotionMode::Vehicle);
            } else {
                commands.entity(entity).insert(LocomotionMode::Vehicle);
            }
            if !has_velocity {
                commands.entity(entity).insert(Velocity::default());
            }
        }
        let authored_view = views.by_subject.get(&source.0).copied().or_else(|| {
            let max_animal = super::tour_view_scoring::authored_maximum_animal_tour_view();
            let static_value = super::tour_view_scoring::authored_static_tour_value();
            let object = definitions.find_object(source.0)?;
            object
                .authored_runtime
                .numeric_behavior_properties
                .iter()
                .any(|&(key, value)| (key == max_animal || key == static_value) && value != 0.0)
                .then_some(source.0)
        });
        match (viewable, authored_view) {
            (Some(mut viewable), Some(definition)) => {
                viewable.set_if_neq(TourViewable { definition });
            }
            (None, Some(definition)) => {
                commands.entity(entity).insert(TourViewable { definition });
            }
            (Some(_), None) => {
                commands.entity(entity).remove::<TourViewable>();
            }
            (None, None) => {}
        }
    }
}

/// Places newly hydrated vehicles on the first stable edge of their circuit.
///
/// Route ownership remains on `TrackSegment` entities; this stores only the
/// vehicle's current edge and scalar position. A vehicle whose circuit has no
/// complete edge remains inert instead of receiving a synthetic route.
pub(super) fn initialize_transport_vehicle_routes_from_first_stable_circuit_edge(
    mut transport_vehicles: Query<
        (
            Entity,
            Ref<CircuitMember>,
            Ref<TransportVehicle>,
            &mut Transform,
        ),
        Without<RoutePosition>,
    >,
    transport_circuits: Query<(Option<&CircuitDirection>, &CircuitMembers), With<TransportCircuit>>,
    track_segments: Query<(Entity, TrackRoute, &TrackTravelDistances)>,
    changed_routes: Query<&CircuitMember, Changed<TrackTravelDistances>>,
    stations: Query<(), With<TransportStation>>,
    mut commands: Commands,
) {
    let changed_circuits: bevy::ecs::entity::EntityHashSet =
        changed_routes.iter().map(|member| member.0).collect();
    for (vehicle_entity, circuit_member, vehicle, mut vehicle_transform) in &mut transport_vehicles
    {
        if !vehicle.is_added()
            && !circuit_member.is_changed()
            && !changed_circuits.contains(&circuit_member.0)
        {
            continue;
        }
        let Ok((direction, members)) = transport_circuits.get(circuit_member.0) else {
            continue;
        };
        let circuit_runs_in_reverse = matches!(direction, Some(CircuitDirection::Reverse));
        let Some((track_segment_entity, track_segment, distances)) = track_segments
            .iter_many(members.iter())
            .filter_map(Result::ok)
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
            distances.travel_length(track_segment.geometry)
        } else {
            0.0
        };
        let (initial_position, _) =
            distances.translation_and_tangent(track_segment.geometry, initial_distance);
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
    pending_cables: Query<(&TrackProfile, Has<TrackTravelDistances>), With<TrackSegment>>,
    mut open: MessageWriter<OpenCircuitRequest>,
    mut commands: Commands,
) {
    for (circuit, membership) in &circuits {
        if membership.iter().all(|member| {
            members.contains(member)
                && pending_cables.get(member).map_or(true, |(profile, ready)| {
                    profile.kind != TransportationTrackKind::Sky || ready
                })
        }) {
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
    ) -> (TrackStart, TrackEnd, TrackSegment, TrackTravelDistances) {
        let points: Box<[Vec3]> = Box::new([from_position, to_position]);
        let (distances, length) = TrackTravelDistances::calculate(&points).unwrap();
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
                length,
            },
            distances,
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
    mut prefab_events: MessageReader<
        AssetEvent<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    >,
    mut prefab_failures: MessageReader<
        bevy::asset::AssetLoadFailedEvent<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    >,
    mut model_events: MessageReader<AssetEvent<bevy::gltf::Gltf>>,
    mut pending: Query<(
        Entity,
        Ref<openzt2_zoo_state::transport_tours::transport_topology_types::RestoredTransportPrefab>,
        &openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
        &openzt2_world_state::world_spawn::world_membership_types::WorldMember,
        &mut DefinitionId,
        Option<&mut Transform>,
        Option<&mut Visibility>,
        Has<TransportVehicle>,
    )>,
) {
    let changed_prefabs = prefab_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            _ => None,
        })
        .chain(prefab_failures.read().map(|failure| failure.id))
        .collect::<bevy::platform::collections::HashSet<_>>();
    let models_changed = model_events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. }
                | AssetEvent::Modified { .. }
                | AssetEvent::LoadedWithDependencies { .. }
        )
    });
    model_events.clear();
    use openzt2_world::world_spawn::prefab_world_instance_spawning::{
        hydrate_loaded_scene_prefab_into_world_instance_root, PrefabWorldInstanceRootValues,
    };
    for (entity, prefab, id, world, definition, pose, visibility, vehicle) in &mut pending {
        if !prefab.is_changed() && !models_changed && !changed_prefabs.contains(&prefab.0.id()) {
            continue;
        }
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
        let definition_id = definition.0;
        let instance_transform = pose.as_deref().copied().unwrap_or_default();
        hydrate_loaded_scene_prefab_into_world_instance_root(
            &mut commands,
            entity,
            PrefabWorldInstanceRootValues {
                transform: pose,
                visibility,
                definition: Some(definition),
            },
            asset,
            prefab.0.clone(),
            world.root,
            definition_id,
            *id,
            instance_transform,
            true,
            None,
            if vehicle {
                avian3d::prelude::RigidBody::Dynamic
            } else {
                avian3d::prelude::RigidBody::Static
            },
        );
        commands.entity(entity).remove::<openzt2_zoo_state::transport_tours::transport_topology_types::RestoredTransportPrefab>();
    }
}
