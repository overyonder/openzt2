use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;

use super::active_model_view_cameras::ActiveModelViewCameras;
use super::prefab_render_layer_camera_matching::PrefabCameraRenderLayers;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;

pub(super) fn orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers(
    cameras: Res<ActiveModelViewCameras>,
    camera_layers: Query<&PrefabCameraRenderLayers>,
    parents: Query<&ChildOf>,
    changed: Query<
        (),
        Or<(
            Changed<Transform>,
            Changed<ChildOf>,
            Changed<PrefabCameraRenderLayers>,
            Changed<PrefabAuthoredBillboardOrientationMode>,
        )>,
    >,
    mut removed_parents: RemovedComponents<ChildOf>,
    mut removed_layers: RemovedComponents<PrefabCameraRenderLayers>,
    mut transforms: ParamSet<(
        TransformHelper,
        Query<Entity, With<PrefabAuthoredBillboardOrientationMode>>,
        Query<(
            Entity,
            &PrefabAuthoredBillboardOrientationMode,
            &mut Transform,
        )>,
    )>,
    mut separated: Local<EntityHashSet>,
) {
    separated.clear();
    separated.extend(removed_parents.read().chain(removed_layers.read()));
    let mut pending = EntityHashMap::default();
    {
        let billboards = transforms.p1();
        for entity in &billboards {
            let mut dirty =
                cameras.is_changed() || changed.contains(entity) || separated.contains(&entity);
            let mut nearest_billboard = None;
            for ancestor in parents.iter_ancestors(entity) {
                dirty |= changed.contains(ancestor) || separated.contains(&ancestor);
                if nearest_billboard.is_none() && billboards.contains(ancestor) {
                    nearest_billboard = Some(ancestor);
                }
            }
            if dirty {
                pending.insert(entity, nearest_billboard);
            }
        }
    }
    let mut frontier = EntityHashSet::default();
    let mut dependents = EntityHashMap::<Vec<Entity>>::default();
    for (&entity, ancestor) in &pending {
        if let Some(parent) = ancestor.filter(|parent| pending.contains_key(parent)) {
            dependents.entry(parent).or_default().push(entity);
        } else {
            frontier.insert(entity);
        }
    }

    // Each layer of this traversal contains independent branches. Compute
    // the next bases after local rotations in the current layer are written.
    let mut bases = EntityHashMap::default();
    let mut next = EntityHashSet::default();
    while !frontier.is_empty() {
        bases.clear();
        next.clear();
        {
            let helper = transforms.p0();
            for &entity in &frontier {
                if let Some(children) = dependents.remove(&entity) {
                    next.extend(children);
                }
                let layers = camera_layers
                    .get(entity)
                    .map(|layers| &layers.0)
                    .unwrap_or_default();
                let Some(camera) = cameras.for_billboard(layers) else {
                    continue;
                };
                let parent = match parents.get(entity) {
                    Ok(parent) => {
                        let Ok(transform) = helper.compute_global_transform(parent.parent()) else {
                            continue;
                        };
                        transform.affine()
                    }
                    Err(_) => Affine3A::IDENTITY,
                };
                bases.insert(entity, (parent, camera.pose));
            }
        }
        transforms
            .p2()
            .par_iter_many_unique_mut(&frontier)
            .for_each(|(entity, mode, mut transform)| {
                let Some(&(parent, camera)) = bases.get(&entity) else {
                    return;
                };
                if let Some(rotation) = billboard_rotation(*mode, *transform, parent, camera) {
                    if transform.rotation != rotation {
                        transform.rotation = rotation;
                    }
                }
            });
        std::mem::swap(&mut frontier, &mut next);
    }
}

fn billboard_rotation(
    mode: PrefabAuthoredBillboardOrientationMode,
    transform: Transform,
    parent: Affine3A,
    camera: GlobalTransform,
) -> Option<Quat> {
    let authored = Transform {
        rotation: mode.authored_local_rotation,
        ..transform
    };
    let authored_world = parent * authored.compute_affine();
    let local_from_world = authored_world.inverse();
    let view_normal = -camera.forward().as_vec3();
    let center_direction = camera.translation() - Vec3::from(authored_world.translation);
    let rotation = if mode.mode == 1 {
        let turn = rotation_about_up_axis(
            local_from_world.transform_vector3(center_direction),
            mode.faces_positive_z,
        )?;
        mode.authored_local_rotation * turn
    } else if matches!(mode.mode, 0 | 2 | 3 | 4) {
        let world_direction = if matches!(mode.mode, 3 | 4) {
            if center_direction.length_squared() < 0.001 {
                return None;
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
            authored_up_axis(mode.faces_positive_z)
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
        mode.authored_local_rotation
            * Quat::from_mat3(&if mode.faces_positive_z {
                Mat3::from_cols(-right, up, direction)
            } else {
                Mat3::from_cols(right, direction, up)
            })
    } else {
        mode.authored_local_rotation
    };
    Some(rotation)
}

/// A NIF billboard turns its local +Y to the camera about its local +Z, in
/// source axes. A BFB billboard's content is lowered to Bevy axes, where its
/// face is local +Z and its up local +Y.
const fn authored_up_axis(faces_positive_z: bool) -> Vec3 {
    if faces_positive_z {
        Vec3::Y
    } else {
        Vec3::Z
    }
}

/// The turn about a billboard's up axis that faces it to `direction`, given
/// in its local space: +Y about +Z for a NIF billboard, +Z about +Y for a BFB.
fn rotation_about_up_axis(direction: Vec3, faces_positive_z: bool) -> Option<Quat> {
    let planar = if faces_positive_z {
        direction.xz()
    } else {
        direction.xy()
    };
    (planar.length_squared() > f32::EPSILON).then(|| {
        if faces_positive_z {
            Quat::from_rotation_y(planar.x.atan2(planar.y))
        } else {
            Quat::from_rotation_z((-planar.x).atan2(planar.y))
        }
    })
}
