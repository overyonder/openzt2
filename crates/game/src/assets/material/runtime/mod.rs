//! Bevy adaptation of evaluated D3D9 effect passes.

pub(crate) mod effect_pass_gpu_data;
pub(crate) mod resolved_effect_transform_bindings;
pub(in crate::assets) mod texture_coordinate_animation;

mod d3d9_shader_and_vertex_layout_mapping;
mod effect_pass_bevy_pipeline_specialization;
pub(super) mod effect_pass_material_asset_construction;
mod effect_pass_uniform_bind_group_preparation;
mod evaluated_d3d9_pass_state;
mod evaluated_d3d9_sampler_state;
mod programmable_vertex_fixed_function_fragment_interface;

#[cfg(test)]
mod evaluated_d3d9_pass_state_tests;
#[cfg(test)]
mod evaluated_d3d9_sampler_state_tests;

use bevy::{pbr::MaterialPlugin, prelude::*, render::storage::ShaderBuffer, shader::Shader};

use effect_pass_gpu_data::{
    D3d9FixedFunctionWorldLightContexts, EffectPassMaterial, FIXED_FUNCTION_WORLD_LIGHT_BUFFER,
};

pub(super) fn register_effect_pass_material_asset_and_fixed_function_shader(application: &mut App) {
    bevy::asset::load_internal_asset!(
        application,
        effect_pass_gpu_data::FIXED_FUNCTION_SHADER,
        "../fixed_function.wgsl",
        Shader::from_wgsl
    );
    application
        .add_plugins(MaterialPlugin::<EffectPassMaterial>::default())
        .init_resource::<texture_coordinate_animation::AnimatedTextureCoordinateMaterials>()
        .add_systems(
            PostUpdate,
            texture_coordinate_animation::advance_texture_coordinate_animations,
        )
        .add_systems(Startup, initialize_fixed_function_world_light_buffer);
}

fn initialize_fixed_function_world_light_buffer(mut buffers: ResMut<Assets<ShaderBuffer>>) {
    buffers
        .insert(
            FIXED_FUNCTION_WORLD_LIGHT_BUFFER.id(),
            ShaderBuffer::from(D3d9FixedFunctionWorldLightContexts::default()),
        )
        .expect("fixed UUID shader-buffer insertion cannot fail");
}
