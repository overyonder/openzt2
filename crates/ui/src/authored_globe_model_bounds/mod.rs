use std::collections::HashSet;

use bevy::{camera::primitives::Aabb, prelude::*};
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;

use super::authored_globe_presentation_types::{UiGlobeMarkerVisual, UiGlobePrimaryRenderable};

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct UiGlobeModelBounds {
    pub(super) minimum: Vec3,
    pub(super) maximum: Vec3,
}

pub(super) fn update_instantiated_globe_model_bounds(
    mut commands: Commands,
    mut model_roots: Query<
        (
            Entity,
            &GlobalTransform,
            Option<Ref<Children>>,
            Option<&mut UiGlobeModelBounds>,
        ),
        (
            With<PrefabModel>,
            Or<(With<UiGlobePrimaryRenderable>, With<UiGlobeMarkerVisual>)>,
        ),
    >,
    changed_meshes_and_local_transforms: Query<
        Entity,
        Or<(
            Changed<Aabb>,
            Changed<Transform>,
            Changed<Children>,
            Changed<ChildOf>,
        )>,
    >,
    primitives: Query<(&Aabb, &GlobalTransform), With<Mesh3d>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
) {
    if model_roots.is_empty() {
        return;
    }
    let mut pending = HashSet::new();
    for (entity, _, descendants, bounds) in model_roots.iter() {
        if descendants
            .as_ref()
            .is_some_and(|children| children.is_changed())
            || (descendants.is_none() && bounds.is_some() && !primitives.contains(entity))
        {
            pending.insert(entity);
        }
    }
    for entity in &changed_meshes_and_local_transforms {
        if let Some(model) = parents
            .iter_ancestors::<ChildOf>(entity)
            .find(|model| model_roots.contains(*model))
        {
            pending.insert(model);
        }
    }
    for entity in pending {
        let Ok((_, root_transform, _, current)) = model_roots.get_mut(entity) else {
            continue;
        };
        let root_from_world = root_transform.affine().inverse();
        let mut bounds = None::<UiGlobeModelBounds>;
        for primitive in
            std::iter::once(entity).chain(children.iter_descendants::<Children>(entity))
        {
            let Ok((local, transform)) = primitives.get(primitive) else {
                continue;
            };
            let root_from_mesh = root_from_world * transform.affine();
            let center = root_from_mesh.transform_point3(local.center.into());
            let half = Vec3::from(local.half_extents);
            let extent = Vec3::from(root_from_mesh.matrix3.x_axis.abs()) * half.x
                + Vec3::from(root_from_mesh.matrix3.y_axis.abs()) * half.y
                + Vec3::from(root_from_mesh.matrix3.z_axis.abs()) * half.z;
            let minimum = center - extent;
            let maximum = center + extent;
            bounds = Some(
                bounds.map_or(UiGlobeModelBounds { minimum, maximum }, |bounds| {
                    UiGlobeModelBounds {
                        minimum: bounds.minimum.min(minimum),
                        maximum: bounds.maximum.max(maximum),
                    }
                }),
            );
        }
        match (current, bounds) {
            (Some(mut current), Some(bounds)) => {
                current.set_if_neq(bounds);
            }
            (None, Some(bounds)) => {
                commands.entity(entity).insert(bounds);
            }
            (Some(_), None) => {
                commands.entity(entity).remove::<UiGlobeModelBounds>();
            }
            (None, None) => {}
        }
    }
}
