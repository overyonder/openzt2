use std::{any::TypeId, sync::Arc};

use bevy::{
    asset::UntypedAssetId,
    core_pipeline::core_3d::Transparent3d,
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    material::labels::DrawFunctionLabel,
    pbr::{
        DrawMaterial, MainPassTransparentDrawFunction, PreparedMaterial, RenderMaterialInstances,
    },
    platform::collections::HashMap,
    prelude::*,
    render::{
        erased_render_asset::ErasedRenderAssets,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, RenderCommand, RenderCommandResult,
            TrackedRenderPass,
        },
        Render, RenderApp, RenderSystems,
    },
};

use super::effect_pass_gpu_data::{EffectPassMaterial, EffectPassPipelineSpecializationKey};

#[derive(Resource, Default)]
pub struct EffectStencilReferences(HashMap<UntypedAssetId, u32>);

pub struct SetEffectStencilReference;

impl<P: PhaseItem> RenderCommand<P> for SetEffectStencilReference {
    type Param = (SRes<EffectStencilReferences>, SRes<RenderMaterialInstances>);
    type ItemQuery = ();
    type ViewQuery = ();

    fn render<'w>(
        item: &P,
        _: (),
        _: Option<()>,
        (references, instances): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(material) = instances.into_inner().instances.get(&item.main_entity()) else {
            return RenderCommandResult::Skip;
        };
        if let Some(&reference) = references.into_inner().0.get(&material.asset_id) {
            pass.set_stencil_reference(reference);
        }
        RenderCommandResult::Success
    }
}

pub type DrawEffectPass = (SetEffectStencilReference, DrawMaterial);

pub(super) fn register(app: &mut App) {
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .init_resource::<EffectStencilReferences>()
            .add_render_command::<Transparent3d, DrawEffectPass>()
            .add_systems(
                Render,
                assign_stencil_reference_draw
                    .in_set(RenderSystems::QueueMeshes)
                    .before(bevy::pbr::queue_material_meshes),
            );
    }
}

fn assign_stencil_reference_draw(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    mut materials: ResMut<ErasedRenderAssets<PreparedMaterial>>,
    mut references: ResMut<EffectStencilReferences>,
) {
    if !materials.is_changed() {
        return;
    }
    let materials = materials.bypass_change_detection();
    references.0.retain(|&id, _| materials.get(id).is_some());
    let draw = draw_functions.read().id::<DrawEffectPass>();
    let transparent = MainPassTransparentDrawFunction.intern();
    for (id, material) in materials.iter_mut() {
        if id.type_id() != TypeId::of::<EffectPassMaterial>()
            || material
                .properties
                .get_draw_function(MainPassTransparentDrawFunction)
                == Some(draw)
        {
            continue;
        }
        // Pipeline specialization borrows these prepared properties. The
        // prepared material owns them when the queue reads the draw label.
        let Some(properties) = Arc::get_mut(&mut material.properties) else {
            continue;
        };
        let key = properties
            .material_key
            .to_key::<EffectPassPipelineSpecializationKey>();
        if key.stencil != Default::default() {
            references.0.insert(id, key.stencil_reference);
            if let Some((_, command)) = properties
                .draw_functions
                .iter_mut()
                .find(|(label, _)| *label == transparent)
            {
                *command = draw;
            }
        } else {
            references.0.remove(&id);
        }
    }
}
