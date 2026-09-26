use bevy::{
    camera::visibility::RenderLayers, ecs::entity::EntityHashSet, math::Affine3A, prelude::*,
};

use crate::plugins::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;

use super::prefab_render_layer_camera_matching::render_layers_for_prefab_entity_and_ancestor_chain;

pub(super) fn orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers(
    cameras: Query<(Entity, &Camera, &GlobalTransform, Option<&RenderLayers>), With<Camera3d>>,
    hierarchy: Query<(Option<&ChildOf>, Option<&RenderLayers>)>,
    global_transforms: Query<Ref<GlobalTransform>>,
    children: Query<&Children>,
    changed_transform_sources: Query<
        Entity,
        (
            Changed<Transform>,
            Without<PrefabAuthoredBillboardOrientationMode>,
        ),
    >,
    directly_changed_billboards: Query<
        Entity,
        Or<(
            Added<PrefabAuthoredBillboardOrientationMode>,
            Changed<PrefabAuthoredBillboardOrientationMode>,
            Changed<ChildOf>,
        )>,
    >,
    mut billboards: Query<(
        Entity,
        &PrefabAuthoredBillboardOrientationMode,
        Option<&ChildOf>,
        &mut Transform,
    )>,
    mut camera_state_buffers: Local<(
        Vec<(Entity, bool, Affine3A, RenderLayers)>,
        Vec<(Entity, bool, Affine3A, RenderLayers)>,
    )>,
    mut billboards_requiring_projection: Local<EntityHashSet>,
) {
    let (previous_camera_states, current_camera_states) = &mut *camera_state_buffers;
    current_camera_states.clear();
    current_camera_states.extend(cameras.iter().map(|(entity, camera, transform, layers)| {
        (
            entity,
            camera.is_active,
            transform.affine(),
            layers.cloned().unwrap_or_default(),
        )
    }));
    let camera_changed = *previous_camera_states != *current_camera_states;
    std::mem::swap(previous_camera_states, current_camera_states);
    billboards_requiring_projection.clear();
    if camera_changed {
        billboards_requiring_projection.extend(billboards.iter().map(|(entity, _, _, _)| entity));
    } else {
        billboards_requiring_projection.extend(&directly_changed_billboards);
        for changed_transform in &changed_transform_sources {
            billboards_requiring_projection.extend(
                children
                    .iter_descendants_depth_first::<Children>(changed_transform)
                    .filter(|entity| billboards.contains(*entity)),
            );
        }
    }
    for entity in billboards_requiring_projection.drain() {
        let Ok((entity, mode, parent, mut transform)) = billboards.get_mut(entity) else {
            continue;
        };
        let parent_transform =
            parent.and_then(|parent| global_transforms.get(parent.parent()).ok());
        let billboard_layers =
            render_layers_for_prefab_entity_and_ancestor_chain(entity, &hierarchy);
        let Some((_, _, camera_transform, _)) =
            cameras.iter().find(|(_, camera, _, camera_layers)| {
                camera.is_active
                    && camera_layers
                        .unwrap_or_default()
                        .intersects(&billboard_layers)
            })
        else {
            continue;
        };
        let camera = camera_transform.translation();
        // Billboard normals face the camera view plane, opposite the camera's
        // forward travel direction. The source SDK describes these vectors as
        // "in line" and the shipped one-sided foliage establishes the sign.
        let camera_facing_direction = -camera_transform.forward().as_vec3();
        let parent_from_world = parent_transform.map(|parent| parent.affine().inverse());
        let target = parent_from_world
            .map(|parent_from_world| parent_from_world.transform_point3(camera))
            .unwrap_or(camera);
        let to_center = target - transform.translation;
        if mode.mode == 1 {
            // Gamebryo's ROTATE_ABOUT_UP adjustment follows the authored
            // local transform. NIF/BFB source local Z is the billboard normal
            // and source local Y is its fixed up axis; after source Y/Z
            // conversion those are Bevy local Y and Z respectively. This mode
            // tracks the camera view direction; pointing at the camera center
            // is the separately authored ALWAYS_FACE_CENTER behavior.
            let parent_local_camera_direction = parent_from_world
                .map(|parent_from_world| {
                    parent_from_world.transform_vector3(camera_facing_direction)
                })
                .unwrap_or(camera_facing_direction);
            let authored_local_direction =
                mode.authored_local_rotation.inverse() * parent_local_camera_direction;
            let direction_in_billboard_rotation_plane =
                Vec2::new(authored_local_direction.x, authored_local_direction.y);
            if direction_in_billboard_rotation_plane.length_squared() > f32::EPSILON {
                let direction = direction_in_billboard_rotation_plane.normalize();
                let radians = (-direction.x).atan2(direction.y);
                transform.rotation = mode.authored_local_rotation * Quat::from_rotation_z(radians);
            }
            continue;
        }
        let parent_local_camera_up = parent_from_world
            .map(|parent_from_world| {
                parent_from_world.transform_vector3(camera_transform.up().as_vec3())
            })
            .unwrap_or(camera_transform.up().as_vec3());
        let direction = if matches!(mode.mode, 3 | 4) {
            to_center
        } else {
            parent_from_world
                .map(|parent_from_world| {
                    parent_from_world.transform_vector3(camera_facing_direction)
                })
                .unwrap_or(camera_facing_direction)
        };
        if direction.length_squared() <= f32::EPSILON {
            continue;
        }
        let direction = direction.normalize();
        if mode.mode == 2 || mode.mode == 4 {
            let up = parent_local_camera_up.reject_from_normalized(direction);
            if up.length_squared() > f32::EPSILON {
                let up = up.normalize();
                let right = direction.cross(up).normalize();
                // Source billboard Z (normal) and Y (up) become Bevy local Y
                // and Z after the one native-coordinate conversion.
                transform.rotation = Quat::from_mat3(&Mat3::from_cols(right, direction, up));
            }
        } else {
            let authored_normal = mode.authored_local_rotation * Vec3::Y;
            transform.rotation =
                Quat::from_rotation_arc(authored_normal, direction) * mode.authored_local_rotation;
        }
    }
}
