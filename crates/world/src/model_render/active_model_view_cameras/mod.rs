use bevy::app::AnimationSystems;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_world_state::{
    model_render::ModelRenderPresentationSet,
    world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection,
};
use std::{cmp::Reverse, collections::BTreeMap};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ActiveModelCamera {
    pub(super) entity: Entity,
    pub(super) pose: GlobalTransform,
    order: isize,
    query_index: usize,
}

#[derive(Default, Resource, PartialEq)]
pub(super) struct ActiveModelViewCameras {
    lod: BTreeMap<usize, ActiveModelCamera>,
    billboard: BTreeMap<usize, ActiveModelCamera>,
}

impl ActiveModelViewCameras {
    pub(super) fn for_level_of_detail(&self, layers: &RenderLayers) -> Option<&ActiveModelCamera> {
        layers
            .iter()
            .filter_map(|layer| self.lod.get(&layer))
            .max_by_key(|camera| (camera.order, Reverse(camera.entity)))
    }

    pub(super) fn for_billboard(&self, layers: &RenderLayers) -> Option<&ActiveModelCamera> {
        layers
            .iter()
            .filter_map(|layer| self.billboard.get(&layer))
            .max_by_key(|camera| (camera.order, camera.query_index))
    }
}

pub(super) fn register_active_model_view_cameras(app: &mut App) {
    app.init_resource::<ActiveModelViewCameras>().add_systems(
        PostUpdate,
        refresh_active_model_view_cameras
            .after(AnimationSystems)
            .before(PrefabBillboardProjection)
            .before(ModelRenderPresentationSet::LevelOfDetail),
    );
}

fn refresh_active_model_view_cameras(
    mut views: ResMut<ActiveModelViewCameras>,
    cameras: Query<(
        Entity,
        Ref<Camera>,
        Ref<Camera3d>,
        Option<Ref<RenderLayers>>,
    )>,
    changed_hierarchy: Query<(), Or<(Changed<Transform>, Changed<ChildOf>)>>,
    parents: Query<&ChildOf>,
    transforms: TransformHelper,
    mut removed_parents: RemovedComponents<ChildOf>,
    mut removed_camera_types: RemovedComponents<Camera3d>,
    mut removed_cameras: RemovedComponents<Camera>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut separated: Local<EntityHashSet>,
) {
    separated.clear();
    separated.extend(removed_parents.read());
    let removed_inputs = (removed_camera_types.read().count() != 0)
        | (removed_cameras.read().count() != 0)
        | (removed_layers
            .read()
            .filter(|&entity| cameras.contains(entity))
            .count()
            != 0);
    if !views.is_added()
        && !removed_inputs
        && !cameras.iter().any(|(entity, camera, kind, layers)| {
            camera.is_changed()
                || kind.is_added()
                || layers.is_some_and(|layers| layers.is_changed())
                || (camera.is_active
                    && std::iter::once(entity)
                        .chain(parents.iter_ancestors(entity))
                        .any(|node| changed_hierarchy.contains(node) || separated.contains(&node)))
        })
    {
        return;
    }

    let mut active = ActiveModelViewCameras::default();
    for (query_index, (entity, camera, _, layers)) in cameras.iter().enumerate() {
        if !camera.is_active {
            continue;
        }
        let Ok(pose) = transforms.compute_global_transform(entity) else {
            continue;
        };
        let view = ActiveModelCamera {
            entity,
            pose,
            order: camera.order,
            query_index,
        };
        for layer in layers.as_deref().unwrap_or_default().iter() {
            let lod = active.lod.entry(layer).or_insert(view);
            if (view.order, Reverse(view.entity)) > (lod.order, Reverse(lod.entity)) {
                *lod = view;
            }
            let billboard = active.billboard.entry(layer).or_insert(view);
            if view.order >= billboard.order {
                *billboard = view;
            }
        }
    }
    views.set_if_neq(active);
}
