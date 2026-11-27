use avian3d::prelude::{RigidBody, SpatialQuery, SpatialQueryFilter};
use bevy::{gltf::Gltf, prelude::*};

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_interface_state::{
    camera::world_pointer_ray_types::WorldPointerRay, ui::pointer_capture_types::UiPointerCapture,
};
use openzt2_world::world_spawn::prefab_world_instance_spawning::{
    hydrate_loaded_scene_prefab_into_world_instance_root, PrefabWorldInstanceRootValues,
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    prefab_transform_conversion::transform_from_authored,
    world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::{
    aquatic::tank_navigation_water::TankNavigationWater,
    locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph,
};

use super::{
    guest_view_navigation_marker_types::{
        GuestViewMarkerSlot, GuestViewNavigationMarker, GuestViewNavigationMarkerOf,
        GuestViewNavigationMarkers, GuestViewProjection,
    },
    immersive_control_execution_types::GuestViewControl,
    immersive_mode_state_types::{GuestViewTileCount, ImmersiveCamera},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn project_guest_view_cursor_and_navigation_markers(
    pointer: Res<WorldPointerRay>,
    spatial: SpatialQuery,
    capture: Res<UiPointerCapture>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    worlds: Query<
        (
            Option<&TankNavigationWater>,
            Option<&ActiveTerrainDerivedNavigationGraph>,
        ),
        With<WorldRoot>,
    >,
    members: Query<&WorldMember>,
    session: Single<
        (
            Entity,
            &ImmersiveCamera,
            &GuestViewProjection,
            &mut GuestViewTileCount,
            Option<&GuestViewNavigationMarkers>,
        ),
        With<GuestViewControl>,
    >,
    parents: Query<&ChildOf>,
    mut visuals: Query<(&GuestViewNavigationMarker, &mut Transform)>,
    mut commands: Commands,
) {
    let (session_entity, camera, scenes, count, existing) = session.into_inner();
    let Ok(member) = members.get(camera.0) else {
        return;
    };
    let world = member.root;
    let Ok((tank_water, navigation)) = worlds.get(world) else {
        return;
    };
    let subject = scenes.excluded_subject;
    let hit_point = pointer.0.filter(|_| !capture.over_ui).and_then(|ray| {
        spatial
            .cast_ray_predicate(
                ray.origin,
                ray.direction,
                f32::MAX,
                false,
                &SpatialQueryFilter::DEFAULT,
                &|entity| {
                    let ancestors =
                        || std::iter::once(entity).chain(parents.iter_ancestors::<ChildOf>(entity));
                    ancestors()
                        .find_map(|ancestor| members.get(ancestor).ok())
                        .is_some_and(|member| member.root == world)
                        && !ancestors()
                            .any(|ancestor| Some(ancestor) == subject || visuals.contains(ancestor))
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
        .filter_map(|hit| Some((GuestViewMarkerSlot::Cursor, scenes.cursor.as_ref()?, hit)))
        .chain(
            positions
                .into_iter()
                .enumerate()
                .filter_map(|(index, position)| {
                    let position = IVec3::from_array(position).as_vec3() * 0.01;
                    Some((
                        GuestViewMarkerSlot::Position(index),
                        scenes.marker.as_ref()?,
                        position,
                    ))
                }),
        );
    let mut unused = existing
        .into_iter()
        .flat_map(|markers| markers.iter())
        .collect::<Vec<_>>();
    for (slot, (definition, handle), position) in targets {
        if let Some(index) = unused.iter().position(|&entity| {
            visuals
                .get(entity)
                .is_ok_and(|(marker, _)| marker.slot == slot && &marker.prefab == handle)
        }) {
            let entity = unused.swap_remove(index);
            if let Ok((marker, transform)) = visuals.get_mut(entity) {
                transform
                    .map_unchanged(|transform| &mut transform.translation)
                    .set_if_neq(position + marker.translation_offset);
            }
            continue;
        }
        let Some(prefab) = prefabs.get(handle) else {
            continue;
        };
        if first_missing_prefab_collider_model_asset_id(prefab, &models).is_some() {
            continue;
        }
        let entity = commands.spawn_empty().id();
        hydrate_loaded_scene_prefab_into_world_instance_root(
            &mut commands,
            entity,
            PrefabWorldInstanceRootValues::default(),
            prefab,
            handle.clone(),
            world,
            *definition,
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
                prefab: handle.clone(),
                translation_offset: transform_from_authored(
                    &prefab.canonical_scene_prefab_document().entities[0].transform,
                )
                .translation
                    + Vec3::Y * prefab.ground_fit_height_offset_m().unwrap_or_default(),
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
