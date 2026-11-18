use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::query::QueryItem;
use bevy::extract::extract_instances::{ExtractInstance, ExtractedInstances};
use bevy::prelude::*;
use bevy::render::{
    batching::NoAutomaticBatching,
    camera::ExtractedCamera,
    render_phase::{PhaseItem, SortedRenderPhase, ViewSortedRenderPhases},
    render_resource::{LoadOp, RenderPassDescriptor, StoreOp},
    renderer::{RenderContext, ViewQuery},
    view::{ExtractedView, RetainedViewEntity, ViewDepthStencilTexture, ViewTarget},
};
use openzt2_world_state::model_render::environment_render_layer::EnvironmentRenderLayer;

#[derive(Component, Clone, Copy)]
pub(super) struct EnvironmentPrimitiveLayer(EnvironmentRenderLayer);

impl ExtractInstance<bevy::render::RenderApp> for EnvironmentPrimitiveLayer {
    type QueryData = &'static Self;
    type QueryFilter = ();
    fn extract(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(*item)
    }
}

pub(super) fn project_environment_layers_to_model_primitives(
    mut commands: Commands,
    changed_roots: Query<Entity, Changed<EnvironmentRenderLayer>>,
    mut branches: MessageReader<super::model_projection_changes::ModelBranchChanged>,
    mut removed_layers: RemovedComponents<EnvironmentRenderLayer>,
    added_meshes: Query<Entity, Added<Mesh3d>>,
    primitives: Query<Option<&EnvironmentPrimitiveLayer>, With<Mesh3d>>,
    hierarchy: Query<(Option<&ChildOf>, Option<&EnvironmentRenderLayer>)>,
    children: Query<&Children>,
    mut visited: Local<EntityHashSet>,
    mut pending: Local<Vec<(Entity, Option<EnvironmentRenderLayer>)>>,
) {
    visited.clear();
    for root in changed_roots
        .iter()
        .chain(removed_layers.read())
        .chain(branches.read().map(|branch| branch.0))
        .chain(added_meshes.iter())
    {
        if visited.contains(&root) {
            continue;
        }
        let Ok((parent, layer)) = hierarchy.get(root) else {
            continue;
        };
        let mut inherited = layer.copied();
        let mut ancestor = parent.map(ChildOf::parent);
        while inherited.is_none() {
            let Some(entity) = ancestor else { break };
            let Ok((parent, layer)) = hierarchy.get(entity) else {
                break;
            };
            inherited = layer.copied();
            ancestor = parent.map(ChildOf::parent);
        }
        pending.push((root, inherited));
        while let Some((entity, layer)) = pending.pop() {
            if !visited.insert(entity) {
                continue;
            }
            if let Ok(descendants) = children.get(entity) {
                pending.extend(descendants.iter().map(|child| {
                    let own_layer = hierarchy
                        .get(child)
                        .ok()
                        .and_then(|(_, layer)| layer.copied());
                    (child, own_layer.or(layer))
                }));
            }
            let Ok(previous) = primitives.get(entity) else {
                continue;
            };
            match layer {
                Some(layer) if previous.is_none_or(|previous| previous.0 != layer) => {
                    // Preserve individual layer boundaries through GPU preparation.
                    commands
                        .entity(entity)
                        .insert((EnvironmentPrimitiveLayer(layer), NoAutomaticBatching));
                }
                None if previous.is_some() => {
                    commands
                        .entity(entity)
                        .remove::<EnvironmentPrimitiveLayer>();
                }
                _ => {}
            }
        }
    }
}

struct PreparedEnvironmentLayer {
    layer: EnvironmentRenderLayer,
    phase: SortedRenderPhase<Transparent3d>,
}

#[derive(Resource, Default)]
pub(super) struct PreparedEnvironmentLayers(
    Vec<(RetainedViewEntity, Vec<PreparedEnvironmentLayer>)>,
);

// Sorted phases are retained. Return last frame's items before Bevy queues and
// retires entities, then move their prepared draws again after batching.
pub(super) fn restore_environment_phase_items_before_queueing(
    mut prepared: ResMut<PreparedEnvironmentLayers>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
) {
    for (view, layers) in prepared.0.drain(..) {
        let Some(phase) = phases.get_mut(&view) else {
            continue;
        };
        for layer in layers {
            for (_, item) in layer.phase.items {
                phase.add_retained(item);
            }
        }
    }
}

pub(super) fn separate_prepared_environment_layers(
    instances: Res<ExtractedInstances<EnvironmentPrimitiveLayer, bevy::render::RenderApp>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut prepared: ResMut<PreparedEnvironmentLayers>,
    mut keys: Local<
        Vec<(
            (Entity, bevy::render::sync_world::MainEntity),
            EnvironmentRenderLayer,
        )>,
    >,
) {
    for (view, phase) in phases.iter_mut() {
        keys.clear();
        keys.extend(phase.items.iter().filter_map(|(key, item)| {
            instances
                .get(&item.main_entity())
                .map(|layer| (*key, layer.0))
        }));
        if keys.is_empty() {
            continue;
        }
        keys.sort_by_key(|(_, layer)| layer.layer);
        let mut layers: Vec<PreparedEnvironmentLayer> = Vec::new();
        for &(key, layer) in keys.iter() {
            let Some(item) = phase.items.shift_remove(&key) else {
                continue;
            };
            if layers
                .last()
                .is_none_or(|previous| previous.layer.layer != layer.layer)
            {
                layers.push(PreparedEnvironmentLayer {
                    layer,
                    phase: default(),
                });
            }
            if let Some(target) = layers.last_mut() {
                target.phase.add_retained(item);
            }
        }
        prepared.0.push((*view, layers));
    }
}

pub(super) fn render_environment_layers_before_world(
    world: &World,
    view: ViewQuery<(
        &ExtractedCamera,
        &ExtractedView,
        &ViewTarget,
        &ViewDepthStencilTexture,
    )>,
    prepared: Res<PreparedEnvironmentLayers>,
    mut context: RenderContext,
) {
    let view_entity = view.entity();
    let (camera, extracted, target, depth) = view.into_inner();
    let Some((_, layers)) = prepared
        .0
        .iter()
        .find(|(view, _)| *view == extracted.retained_view_entity)
    else {
        return;
    };
    let mut clear_depth = false;
    for layer in layers {
        // These attachments share Bevy's first-use clear state with the world
        // passes. Drawing the sky first consumes their normal initial clear.
        let mut depth_attachment = depth.get_attachment(StoreOp::Store);
        if clear_depth {
            if let Some(operations) = &mut depth_attachment.depth_ops {
                operations.load = LoadOp::Clear(0.0);
            }
        }
        let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("authored environment layer"),
            color_attachments: &[Some(target.get_color_attachment())],
            depth_stencil_attachment: Some(depth_attachment),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(viewport) = &camera.viewport {
            pass.set_camera_viewport(viewport);
        }
        if let Err(error) = layer.phase.render(&mut pass, world, view_entity) {
            error!(?error, "environment layer draw failed");
        }
        clear_depth = layer.layer.clear_depth_after;
    }
    if clear_depth {
        let mut attachment = depth.get_attachment(StoreOp::Store);
        if let Some(operations) = &mut attachment.depth_ops {
            operations.load = LoadOp::Clear(0.0);
        }
        let _pass = context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("authored environment depth boundary"),
            color_attachments: &[],
            depth_stencil_attachment: Some(attachment),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}
