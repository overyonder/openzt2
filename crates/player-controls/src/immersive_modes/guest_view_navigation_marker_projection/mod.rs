use avian3d::prelude::{RigidBody, SpatialQuery, SpatialQueryFilter};
use bevy::{gltf::Gltf, prelude::*};

use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    world_membership_types::WorldRoot,
};
use openzt2_zoo_state::{
    aquatic::tank_navigation_water::TankNavigationWater,
    locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph,
};

use super::{
    guest_view_navigation_marker_types::{
        GuestViewMarkerSlot, GuestViewNavigationMarker, GuestViewNavigationMarkerOf,
        GuestViewNavigationMarkers,
    },
    immersive_control_execution_types::{GuestViewControl, GuestViewTileCount},
    immersive_mode_entry_and_policy_operations::find_authored_immersive_mode_policy,
    immersive_mode_state_types::{ImmersiveMode, ImmersiveSubject},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn project_guest_view_cursor_and_navigation_markers(
    pointer: Res<WorldPointerRay>,
    spatial: SpatialQuery,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    world: Single<(Entity, Option<&TankNavigationWater>), With<WorldRoot>>,
    navigation: Option<Single<&ActiveTerrainDerivedNavigationGraph>>,
    session: Single<
        (
            Entity,
            &ImmersiveSubject,
            &mut GuestViewTileCount,
            Option<&GuestViewNavigationMarkers>,
        ),
        With<GuestViewControl>,
    >,
    parents: Query<&ChildOf>,
    mut visuals: Query<(&GuestViewNavigationMarker, &mut Transform)>,
    mut commands: Commands,
) {
    let (world, tank_water) = world.into_inner();
    let (session_entity, subject, count, existing) = session.into_inner();
    let definitions = definitions.get(&definition_assets);
    let policy = definitions.and_then(|definitions| {
        find_authored_immersive_mode_policy(definitions, ImmersiveMode::GuestView)
    });
    let hit_point = pointer.0.and_then(|ray| {
        spatial
            .cast_ray_predicate(
                ray.origin,
                ray.direction,
                f32::MAX,
                false,
                &SpatialQueryFilter::DEFAULT,
                &|entity| {
                    entity != session_entity
                        && entity != subject.0
                        && !visuals.contains(entity)
                        && !parents
                            .iter_ancestors::<ChildOf>(entity)
                            .any(|ancestor| ancestor == subject.0 || visuals.contains(ancestor))
                },
            )
            .map(|hit| ray.origin + *ray.direction * hit.distance)
    });

    let mut positions = Vec::new();
    if let (Some(hit), Some(navigation)) = (hit_point, navigation) {
        let position_cm = (hit * 100.0).round().as_ivec3().to_array();
        positions.extend(navigation.guest_view_marker_positions(position_cm, tank_water));
    }
    count
        .map_unchanged(|count| &mut count.0)
        .set_if_neq(positions.len() as u32);

    let targets = hit_point
        .into_iter()
        .filter_map(|hit| {
            Some((
                GuestViewMarkerSlot::Cursor,
                policy?.guest_ground_cursor,
                hit,
            ))
        })
        .chain(
            positions
                .into_iter()
                .enumerate()
                .filter_map(|(index, position)| {
                    let position = IVec3::from_array(position).as_vec3() * 0.01;
                    Some((
                        GuestViewMarkerSlot::Position(index),
                        policy?.guest_navigation_marker,
                        position,
                    ))
                }),
        );
    let mut unused = existing
        .into_iter()
        .flat_map(|markers| markers.iter())
        .collect::<Vec<_>>();
    for (slot, definition, position) in targets {
        let Some(handle) = definitions.and_then(|definitions| definitions.scene(definition)) else {
            continue;
        };
        if let Some(index) = unused.iter().position(|&entity| {
            visuals
                .get(entity)
                .is_ok_and(|(marker, _)| marker.slot == slot && marker.prefab == handle)
        }) {
            let entity = unused.swap_remove(index);
            if let Ok((_, transform)) = visuals.get_mut(entity) {
                transform
                    .map_unchanged(|transform| &mut transform.translation)
                    .set_if_neq(position);
            }
            continue;
        }
        let Some(prefab) = prefabs.get(&handle) else {
            continue;
        };
        if first_missing_prefab_collider_model_asset_id(prefab, &models).is_some() {
            continue;
        }
        let entity = commands.spawn_empty().id();
        openzt2_world::world_spawn::prefab_world_instance_spawning::hydrate_loaded_scene_prefab_into_world_instance_root(
            &mut commands,
            entity,
            prefab,
            handle.clone(),
            world,
            definition,
            PersistentId(u64::MAX - entity.to_bits()),
            Transform::from_translation(position),
            true,
            None,
            RigidBody::Static,
        );
        commands.entity(entity).insert((
            GuestViewNavigationMarkerOf(session_entity),
            GuestViewNavigationMarker {
                slot,
                prefab: handle,
            },
        ));
    }
    for entity in unused {
        commands.entity(entity).try_despawn();
    }
}

pub(super) fn drop_navigation_markers_when_guest_view_ends(
    removed: On<Remove<GuestViewControl>>,
    markers: Query<&GuestViewNavigationMarkers>,
    mut commands: Commands,
) {
    if let Ok(markers) = markers.get(removed.entity) {
        for entity in markers.iter() {
            commands.entity(entity).try_despawn();
        }
    }
}
