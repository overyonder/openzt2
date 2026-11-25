use bevy::app::AnimationSystems;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_world_state::{
    model_render::ModelRenderPresentationSet,
    world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection,
};
use std::collections::BTreeMap;

#[derive(Component, Clone, Copy)]
pub struct PrefabLodCameraViewFactor(pub f32);

#[derive(Component, Clone, PartialEq)]
struct ModelCameraViewInput {
    pose: GlobalTransform,
    order: isize,
    layers: RenderLayers,
    lod_view_factor: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ActiveModelCamera {
    pub(super) entity: Entity,
    pub(super) pose: GlobalTransform,
    pub(super) lod_view_factor: f32,
    order: isize,
    query_index: usize,
}

#[derive(Default, Resource, PartialEq)]
pub(super) struct ActiveModelViewCameras {
    pub(super) views: Vec<(ActiveModelCamera, RenderLayers)>,
    billboard: BTreeMap<usize, ActiveModelCamera>,
}

impl ActiveModelViewCameras {
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
        (
            publish_model_camera_view_inputs.after(AnimationSystems),
            refresh_active_model_view_cameras,
        )
            .chain()
            .before(PrefabBillboardProjection)
            .before(ModelRenderPresentationSet::LevelOfDetail),
    );
}

pub(super) fn publish_model_camera_view_inputs(
    mut commands: Commands,
    mut cameras: Query<
        (
            Entity,
            Ref<Camera>,
            Option<Ref<RenderLayers>>,
            Option<Ref<PrefabLodCameraViewFactor>>,
            Option<&mut ModelCameraViewInput>,
        ),
        With<Camera3d>,
    >,
    changed_hierarchy: Query<(), Or<(Changed<Transform>, Changed<ChildOf>)>>,
    parents: Query<&ChildOf>,
    transforms: TransformHelper,
    mut removed_parents: RemovedComponents<ChildOf>,
    mut removed_camera_types: RemovedComponents<Camera3d>,
    mut removed_cameras: RemovedComponents<Camera>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut removed_factors: RemovedComponents<PrefabLodCameraViewFactor>,
) {
    let separated = removed_parents.read().collect::<EntityHashSet>();
    let removed_layers = removed_layers
        .read()
        .chain(removed_factors.read())
        .collect::<EntityHashSet>();
    for entity in removed_camera_types.read().chain(removed_cameras.read()) {
        if !cameras.contains(entity) {
            commands.entity(entity).try_remove::<ModelCameraViewInput>();
        }
    }
    for (entity, camera, layers, factor, input) in &mut cameras {
        if !camera.is_active {
            if input.is_some() {
                commands.entity(entity).try_remove::<ModelCameraViewInput>();
            }
            continue;
        }
        let moved = std::iter::once(entity)
            .chain(parents.iter_ancestors::<ChildOf>(entity))
            .any(|node| changed_hierarchy.contains(node) || separated.contains(&node));
        if input.is_some()
            && !moved
            && !camera.is_changed()
            && !layers.as_ref().is_some_and(Ref::is_changed)
            && !factor.as_ref().is_some_and(Ref::is_changed)
            && !removed_layers.contains(&entity)
        {
            continue;
        }
        let pose = if let Some(input) = input.as_ref().filter(|_| !moved) {
            input.pose
        } else {
            let Ok(pose) = transforms.compute_global_transform(entity) else {
                continue;
            };
            pose
        };
        let next = ModelCameraViewInput {
            pose,
            order: camera.order,
            layers: layers.as_deref().cloned().unwrap_or_default(),
            lod_view_factor: factor
                .filter(|factor| factor.0.is_finite() && factor.0 > 0.0)
                .map_or(1.0, |factor| factor.0),
        };
        if let Some(mut input) = input {
            input.set_if_neq(next);
        } else {
            commands.entity(entity).try_insert(next);
        }
    }
}

fn refresh_active_model_view_cameras(
    mut views: ResMut<ActiveModelViewCameras>,
    cameras: Query<(Entity, &ModelCameraViewInput)>,
    changed: Query<(), Changed<ModelCameraViewInput>>,
    mut removed: RemovedComponents<ModelCameraViewInput>,
) {
    if removed.read().count() == 0 && changed.is_empty() {
        return;
    }
    let mut active = ActiveModelViewCameras::default();
    for (query_index, (entity, input)) in cameras.iter().enumerate() {
        let view = ActiveModelCamera {
            entity,
            pose: input.pose,
            lod_view_factor: input.lod_view_factor,
            order: input.order,
            query_index,
        };
        for layer in input.layers.iter() {
            let billboard = active.billboard.entry(layer).or_insert(view);
            if view.order >= billboard.order {
                *billboard = view;
            }
        }
        active.views.push((view, input.layers.clone()));
    }
    views.set_if_neq(active);
}
