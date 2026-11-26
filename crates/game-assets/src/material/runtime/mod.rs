//! Bevy adaptation of evaluated D3D9 effect passes.

pub mod effect_pass_gpu_data;
pub mod resolved_effect_transform_bindings;
pub mod texture_coordinate_animation;

pub mod d3d9_shader_and_vertex_layout_mapping;
pub mod effect_pass_bevy_pipeline_specialization;
pub mod effect_pass_material_asset_construction;
pub mod effect_pass_stencil_reference;
pub mod effect_pass_uniform_bind_group_preparation;
pub mod evaluated_d3d9_pass_state;
pub mod evaluated_d3d9_sampler_state;
mod gpu_semantic_shader_registers;
mod gpu_texture_transforms;
mod morph_fragment_opacity;
pub mod programmable_vertex_fixed_function_fragment_interface;

#[cfg(test)]
mod evaluated_d3d9_pass_state_tests;
#[cfg(test)]
mod evaluated_d3d9_sampler_state_tests;

use bevy::{
    asset::AssetEvent,
    camera::Camera3d,
    ecs::schedule::SystemCondition,
    pbr::MaterialPlugin,
    prelude::*,
    render::{renderer::RenderDevice, storage::ShaderBuffer},
    shader::Shader,
};

use effect_pass_gpu_data::{
    D3d9FixedFunctionWorldLightContexts, EffectEnvironmentRegisters, EffectPassMaterial,
    EFFECT_ENVIRONMENT_REGISTER_BUFFER, FIXED_FUNCTION_WORLD_LIGHT_BUFFER,
};
use effect_pass_stencil_reference::{
    refresh_effect_pass_depth_stencil_format, select_camera_depth_stencil_format,
    EffectPassDepthStencilFormat,
};

pub(super) fn register_effect_pass_material_asset_and_fixed_function_shader(application: &mut App) {
    bevy::asset::load_internal_asset!(
        application,
        effect_pass_gpu_data::FIXED_FUNCTION_SHADER,
        "../fixed_function.wesl",
        |source, _| Shader::from_wesl(
            morph_fragment_opacity::fixed_function_source_with_morph_opacity(source),
            "embedded://openzt2/fixed_function.wesl",
        )
    );
    application
        .add_plugins(MaterialPlugin::<EffectPassMaterial>::default())
        .init_resource::<EffectPassDepthStencilFormat>()
        .add_systems(
            Startup,
            (
                initialize_fixed_function_world_light_buffer,
                refresh_effect_pass_depth_stencil_format,
            ),
        )
        .add_systems(
            Last,
            (
                refresh_effect_pass_depth_stencil_format.run_if(
                    on_message::<AssetEvent<EffectPassMaterial>>
                        .or_eager(resource_changed_or_removed::<RenderDevice>),
                ),
                select_camera_depth_stencil_format
                    .after(refresh_effect_pass_depth_stencil_format)
                    .run_if(
                        resource_changed::<EffectPassDepthStencilFormat>
                            .or_eager(any_match_filter::<Added<Camera3d>>),
                    ),
            ),
        );
    texture_coordinate_animation::register_texture_coordinate_animation(application);
}

fn initialize_fixed_function_world_light_buffer(mut buffers: ResMut<Assets<ShaderBuffer>>) {
    let mut buffer = ShaderBuffer::default();
    D3d9FixedFunctionWorldLightContexts::default().write_shader_buffer(&mut buffer);
    buffers
        .insert(FIXED_FUNCTION_WORLD_LIGHT_BUFFER.id(), buffer)
        .expect("fixed UUID shader-buffer insertion cannot fail");
    let mut buffer = ShaderBuffer::default();
    EffectEnvironmentRegisters::default().write_shader_buffer(&mut buffer);
    buffers
        .insert(EFFECT_ENVIRONMENT_REGISTER_BUFFER.id(), buffer)
        .expect("fixed UUID shader-buffer insertion cannot fail");
}
