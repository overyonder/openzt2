use bevy::{
    camera::{Camera3d, DepthStencilFormat},
    pbr::DrawMaterial,
    prelude::*,
    render::{render_resource::WgpuFeatures, renderer::RenderDevice},
};

use super::effect_pass_gpu_data::EffectPassMaterial;

pub type DrawEffectPass = DrawMaterial;

#[derive(Resource, Default, PartialEq)]
pub(super) struct EffectPassDepthStencilFormat(DepthStencilFormat);

pub(super) fn refresh_effect_pass_depth_stencil_format(
    materials: Res<Assets<EffectPassMaterial>>,
    device: Option<Res<RenderDevice>>,
    mut requested: ResMut<EffectPassDepthStencilFormat>,
) {
    let requires_stencil = materials.iter().any(|(_, material)| {
        material
            .pipeline_specialization_key
            .stencil_reference
            .is_some()
    });
    let format = if !requires_stencil {
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
    requested.set_if_neq(EffectPassDepthStencilFormat(format));
}

pub(super) fn select_camera_depth_stencil_format(
    requested: Res<EffectPassDepthStencilFormat>,
    mut cameras: ParamSet<(
        Query<&mut DepthStencilFormat, With<Camera3d>>,
        Query<&mut DepthStencilFormat, Added<Camera3d>>,
    )>,
) {
    if requested.is_changed() {
        for mut format in &mut cameras.p0() {
            format.set_if_neq(requested.0);
        }
    } else {
        for mut format in &mut cameras.p1() {
            format.set_if_neq(requested.0);
        }
    }
}
