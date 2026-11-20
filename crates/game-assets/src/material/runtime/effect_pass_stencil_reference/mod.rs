use bevy::{
    asset::AssetEvent,
    camera::{Camera3d, DepthStencilFormat},
    pbr::DrawMaterial,
    prelude::*,
    render::{render_resource::WgpuFeatures, renderer::RenderDevice},
};

use super::effect_pass_gpu_data::EffectPassMaterial;

pub type DrawEffectPass = DrawMaterial;

pub(super) fn select_camera_depth_stencil_format(
    materials: Res<Assets<EffectPassMaterial>>,
    mut material_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    device: Option<Res<RenderDevice>>,
    mut stencil_required: Local<Option<bool>>,
    mut cameras: Query<&mut DepthStencilFormat, With<Camera3d>>,
) {
    if stencil_required.is_none() || !material_events.is_empty() {
        material_events.clear();
        *stencil_required = Some(materials.iter().any(|(_, material)| {
            material
                .pipeline_specialization_key
                .stencil_reference
                .is_some()
        }));
    }

    let format = if *stencil_required != Some(true) {
        DepthStencilFormat::Depth32Float
    } else if device.is_some_and(|device| {
        device
            .features()
            .contains(WgpuFeatures::DEPTH32FLOAT_STENCIL8)
    }) {
        DepthStencilFormat::Depth32FloatStencil8
    } else {
        DepthStencilFormat::Depth24PlusStencil8
    };

    for mut camera_format in &mut cameras {
        camera_format.set_if_neq(format);
    }
}
