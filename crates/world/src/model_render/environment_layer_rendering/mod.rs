use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d};
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::query::QueryItem;
use bevy::extract::extract_instances::{ExtractInstance, ExtractedInstances};
use bevy::pbr::{
    get_mesh_instance_world_from_local, EntitiesNeedingSpecialization, MeshInputUniform,
    MeshUniform, PreparedMaterial, RenderMaterialInstances, RenderMeshInstances,
    SpecializedMaterialPipelineCache,
};
use bevy::prelude::*;
use bevy::render::{
    batching::gpu_preprocessing::BatchedInstanceBuffers,
    camera::ExtractedCamera,
    erased_render_asset::ErasedRenderAssets,
    mesh::{allocator::MeshAllocator, RenderMesh},
    render_asset::RenderAssets,
    render_phase::{
        CachedRenderPipelinePhaseItem, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
        SortedPhaseItem, ViewSortedRenderPhases,
    },
    render_resource::{LoadOp, RenderPassDescriptor, StoreOp},
    renderer::{RenderContext, ViewQuery},
    sync_world::MainEntity,
    view::{ExtractedView, RenderVisibleEntities, ViewDepthStencilTexture, ViewTarget},
};
use openzt2_game_assets::material::runtime::{
    effect_pass_gpu_data::EffectPassMaterial, effect_pass_stencil_reference::DrawEffectPass,
};
use openzt2_world_state::model_render::{
    authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder,
    environment_render_layer::EnvironmentRenderLayer,
};

use super::authored_effect_technique_pass_submission_order::{
    transparent_draw_sort_key, TransparentDrawSortKey,
};

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct EnvironmentPrimitiveLayer(EnvironmentRenderLayer);

impl ExtractInstance<bevy::render::RenderApp> for EnvironmentPrimitiveLayer {
    type QueryData = &'static Self;
    type QueryFilter = With<Mesh3d>;
    fn extract(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(*item)
    }
}

pub(super) fn register_inherited_environment_render_layers(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<EnvironmentPrimitiveLayer>::new(PostUpdate))
        .add_systems(
            PostUpdate,
            (
                publish_environment_render_layer_sources
                    .after(super::authored_model_material_pass_projection::project_authored_model_material_passes)
                    .before(PropagateSet::<EnvironmentPrimitiveLayer>::default()),
                requeue_world_draws_after_environment_layer_removal::<StandardMaterial>
                    .after(PropagateSet::<EnvironmentPrimitiveLayer>::default())
                    .after(bevy::pbr::check_entities_needing_specialization::<StandardMaterial>),
                requeue_world_draws_after_environment_layer_removal::<EffectPassMaterial>
                    .after(PropagateSet::<EnvironmentPrimitiveLayer>::default())
                    .after(bevy::pbr::check_entities_needing_specialization::<EffectPassMaterial>),
            ),
        );
}

fn publish_environment_render_layer_sources(
    mut commands: Commands,
    changed: Query<(Entity, &EnvironmentRenderLayer), Changed<EnvironmentRenderLayer>>,
    mut removed: RemovedComponents<EnvironmentRenderLayer>,
    mut sources: Query<&mut Propagate<EnvironmentPrimitiveLayer>>,
) {
    for (entity, &layer) in &changed {
        let source = Propagate(EnvironmentPrimitiveLayer(layer));
        if let Ok(mut current) = sources.get_mut(entity) {
            current.set_if_neq(source);
        } else {
            commands.entity(entity).insert(source);
        }
    }
    for entity in removed.read() {
        if !changed.contains(entity) {
            if let Ok(mut target) = commands.get_entity(entity) {
                target.remove::<Propagate<EnvironmentPrimitiveLayer>>();
            }
        }
    }
}

fn requeue_world_draws_after_environment_layer_removal<M: bevy::pbr::Material>(
    mut removed: RemovedComponents<EnvironmentPrimitiveLayer>,
    ordinary_meshes: Query<(), (With<MeshMaterial3d<M>>, Without<EnvironmentPrimitiveLayer>)>,
    mut specializations: ResMut<EntitiesNeedingSpecialization<M>>,
) {
    specializations.changed.extend(
        removed
            .read()
            .filter(|&entity| ordinary_meshes.contains(entity)),
    );
}

pub(super) struct EnvironmentPhaseItem {
    layer: EnvironmentRenderLayer,
    order: Option<AuthoredEffectTechniquePassSubmissionOrder>,
    sort_key: TransparentDrawSortKey,
    draw: Transparent3d,
}

impl PhaseItem for EnvironmentPhaseItem {
    // A batch cannot span a depth-clear boundary between environment layers.
    const AUTOMATIC_BATCHING: bool = false;

    fn entity(&self) -> Entity {
        self.draw.entity()
    }
    fn main_entity(&self) -> MainEntity {
        self.draw.main_entity()
    }
    fn draw_function(&self) -> bevy::material::labels::DrawFunctionId {
        self.draw.draw_function()
    }
    fn batch_range(&self) -> &std::ops::Range<u32> {
        self.draw.batch_range()
    }
    fn batch_range_mut(&mut self) -> &mut std::ops::Range<u32> {
        self.draw.batch_range_mut()
    }
    fn extra_index(&self) -> PhaseItemExtraIndex {
        self.draw.extra_index()
    }
    fn batch_range_and_extra_index_mut(
        &mut self,
    ) -> (&mut std::ops::Range<u32>, &mut PhaseItemExtraIndex) {
        self.draw.batch_range_and_extra_index_mut()
    }
}

impl CachedRenderPipelinePhaseItem for EnvironmentPhaseItem {
    fn cached_pipeline(&self) -> bevy::render::render_resource::CachedRenderPipelineId {
        self.draw.cached_pipeline()
    }
}

impl SortedPhaseItem for EnvironmentPhaseItem {
    type SortKey = (u16, TransparentDrawSortKey);

    fn sort_key(&self) -> Self::SortKey {
        (self.layer.layer, self.sort_key)
    }

    fn sort(
        items: &mut indexmap::IndexMap<(Entity, MainEntity), Self, bevy::ecs::entity::EntityHash>,
    ) {
        items.sort_by_key(|_, item| item.sort_key());
    }

    fn recalculate_sort_keys(
        items: &mut indexmap::IndexMap<(Entity, MainEntity), Self, bevy::ecs::entity::EntityHash>,
        _: &ExtractedView,
    ) {
        let mut invocation_distances = EntityHashMap::default();
        for item in items.values() {
            if let Some(order) = item.order {
                order.record_invocation_distance(&mut invocation_distances, item.draw.distance);
            }
        }
        for item in items.values_mut() {
            item.sort_key = transparent_draw_sort_key(
                item.order.as_ref(),
                item.draw.distance,
                &invocation_distances,
            );
        }
    }

    fn indexed(&self) -> bool {
        self.draw.indexed
    }
}

pub(super) fn queue_environment_layers(
    layers: Res<ExtractedInstances<EnvironmentPrimitiveLayer, bevy::render::RenderApp>>,
    orders: Res<
        ExtractedInstances<AuthoredEffectTechniquePassSubmissionOrder, bevy::render::RenderApp>,
    >,
    mut phases: ResMut<ViewSortedRenderPhases<EnvironmentPhaseItem>>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    material_instances: Res<RenderMaterialInstances>,
    mesh_instances: Res<RenderMeshInstances>,
    mesh_assets: Res<RenderAssets<RenderMesh>>,
    mesh_allocator: Res<MeshAllocator>,
    buffers: Option<Res<BatchedInstanceBuffers<MeshUniform, MeshInputUniform>>>,
    pipelines: Res<SpecializedMaterialPipelineCache>,
    draws: Res<DrawFunctions<EnvironmentPhaseItem>>,
    views: Query<(&ExtractedView, &RenderVisibleEntities)>,
) {
    let draw_function = draws.read().id::<DrawEffectPass>();
    phases.retain(|retained, _| {
        views
            .iter()
            .any(|(view, _)| view.retained_view_entity == *retained)
    });
    for (view, visibility) in &views {
        let view_id = view.retained_view_entity;
        let (Some(visible), Some(pipelines)) =
            (visibility.get::<Mesh3d>(), pipelines.get(&view_id))
        else {
            phases.remove(&view_id);
            continue;
        };
        phases.prepare_for_new_frame(view_id);
        let phase = phases.get_mut(&view_id).unwrap();
        let rangefinder = view.rangefinder3d();
        let mut queued = bevy::render::sync_world::MainEntityHashSet::default();
        for (_, entity) in visible.iter_visible() {
            let Some(layer) = layers.get(entity) else {
                continue;
            };
            let Some(&pipeline) = pipelines.get(entity) else {
                continue;
            };
            let Some(material) = material_instances
                .instances
                .get(entity)
                .and_then(|instance| materials.get(instance.asset_id))
            else {
                continue;
            };
            if !matches!(
                material.properties.alpha_mode,
                bevy::material::AlphaMode::Blend
                    | bevy::material::AlphaMode::Premultiplied
                    | bevy::material::AlphaMode::Add
                    | bevy::material::AlphaMode::Multiply
            ) {
                continue;
            }
            let Some(mesh) = mesh_instances.render_mesh_queue_data(*entity) else {
                continue;
            };
            let mesh_id = mesh.mesh_asset_id();
            let Some(asset) = mesh_assets.get(mesh_id) else {
                continue;
            };
            let Some(slabs) = mesh_allocator.mesh_slabs(&mesh_id) else {
                continue;
            };
            let centre = get_mesh_instance_world_from_local(
                *entity,
                mesh.current_uniform_index,
                &mesh_instances,
                buffers.as_deref(),
            )
            .transform_point3(asset.aabb_center);
            let sorting_info = TransparentSortingInfo3d::Sorted {
                mesh_center: centre,
                depth_bias: material.properties.depth_bias,
            };
            if queued.insert(*entity) {
                phase.add_retained(EnvironmentPhaseItem {
                    layer: layer.0,
                    order: orders.get(entity).copied(),
                    sort_key: default(),
                    draw: Transparent3d {
                        entity: (Entity::PLACEHOLDER, *entity),
                        draw_function,
                        pipeline,
                        batch_range: 0..1,
                        extra_index: PhaseItemExtraIndex::None,
                        indexed: slabs.index_slab_id.is_some(),
                        distance: sorting_info.sort_distance(&rangefinder),
                        sorting_info,
                    },
                });
            }
        }
        phase.items.retain(|(_, entity), _| queued.contains(entity));
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
    phases: Res<ViewSortedRenderPhases<EnvironmentPhaseItem>>,
    mut context: RenderContext,
) {
    let view_entity = view.entity();
    let (camera, extracted, target, depth) = view.into_inner();
    let Some(phase) = phases.get(&extracted.retained_view_entity) else {
        return;
    };
    let mut clear_depth = false;
    let mut start = 0;
    while let Some((_, item)) = phase.items.get_index(start) {
        let layer = item.layer;
        let end = start
            + phase
                .items
                .values()
                .skip(start)
                .take_while(|item| item.layer.layer == layer.layer)
                .count();
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
        if let Err(error) = phase.render_range(&mut pass, world, view_entity, start..end) {
            error!(?error, "environment layer draw failed");
        }
        clear_depth = layer.clear_depth_after;
        start = end;
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
