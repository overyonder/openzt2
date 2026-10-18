use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;

use crate::model_render::prefab_render_layer_camera_matching::render_layers_for_prefab_entity_and_ancestor_chain;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;

pub(super) fn orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers(
    cameras: Query<(Entity, &Camera, Option<&RenderLayers>), With<Camera3d>>,
    hierarchy: Query<(
        Option<&ChildOf>,
        Option<&RenderLayers>,
        Option<&crate::model_render::world_entity_model_suppression::SuppressedModelRenderLayers>,
    )>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(
        TransformHelper,
        Query<(
            Entity,
            &PrefabAuthoredBillboardOrientationMode,
            &Transform,
            Option<&ChildOf>,
        )>,
        Query<&mut Transform>,
    )>,
    mut ordered_billboards: Local<Vec<(usize, Entity)>>,
) {
    ordered_billboards.clear();
    ordered_billboards.extend(
        transforms
            .p1()
            .iter()
            .map(|(entity, ..)| (parents.iter_ancestors(entity).count(), entity)),
    );
    ordered_billboards.sort_unstable();
    // Parent billboards must settle before their children's authored world bases
    // are calculated. Publish local rotations before Bevy propagates the tree.
    for &(_, entity) in ordered_billboards.iter() {
        let (mode, transform, parent) = {
            let query = transforms.p1();
            let Ok((_, mode, transform, parent)) = query.get(entity) else {
                continue;
            };
            (*mode, *transform, parent.map(ChildOf::parent))
        };
        let layers = render_layers_for_prefab_entity_and_ancestor_chain(entity, &hierarchy);
        let Some((camera, ..)) = cameras.iter().find(|(_, camera, camera_layers)| {
            camera.is_active && camera_layers.unwrap_or_default().intersects(&layers)
        }) else {
            continue;
        };
        let helper = transforms.p0();
        let Ok(camera) = helper.compute_global_transform(camera) else {
            continue;
        };
        let parent = match parent {
            Some(parent) => {
                let Ok(parent) = helper.compute_global_transform(parent) else {
                    continue;
                };
                parent.affine()
            }
            None => bevy::math::Affine3A::IDENTITY,
        };
        let authored = Transform {
            rotation: mode.authored_local_rotation,
            ..transform
        };
        let authored_world = parent * authored.compute_affine();
        let local_from_world = authored_world.inverse();
        let view_normal = -camera.forward().as_vec3();
        let center_direction = camera.translation() - Vec3::from(authored_world.translation);
        let rotation = if mode.mode == 1 {
            let direction = local_from_world.transform_vector3(center_direction).xy();
            if direction.length_squared() <= f32::EPSILON {
                continue;
            }
            mode.authored_local_rotation * Quat::from_rotation_z((-direction.x).atan2(direction.y))
        } else if matches!(mode.mode, 0 | 2 | 3 | 4) {
            let world_direction = if matches!(mode.mode, 3 | 4) {
                if center_direction.length_squared() < 0.001 {
                    continue;
                }
                center_direction.normalize()
            } else {
                view_normal
            };
            let direction = local_from_world
                .transform_vector3(world_direction)
                .normalize();
            let up = if matches!(mode.mode, 2 | 4) {
                let camera_up = if mode.mode == 4 {
                    Quat::from_rotation_arc(view_normal, world_direction) * camera.up().as_vec3()
                } else {
                    camera.up().as_vec3()
                };
                local_from_world.transform_vector3(camera_up)
            } else {
                // Non-rigid facing retains the authored up axis's projection.
                Vec3::Z
            };
            let up = up.reject_from_normalized(direction);
            let right = if up.length_squared() > f32::EPSILON {
                direction.cross(up.normalize()).normalize()
            } else {
                -local_from_world
                    .transform_vector3(camera.up().as_vec3())
                    .normalize()
            };
            let up = right.cross(direction).normalize();
            mode.authored_local_rotation * Quat::from_mat3(&Mat3::from_cols(right, direction, up))
        } else {
            mode.authored_local_rotation
        };
        let mut query = transforms.p2();
        if let Ok(mut target) = query.get_mut(entity) {
            if target.rotation != rotation {
                target.rotation = rotation;
            }
        }
    }
}
