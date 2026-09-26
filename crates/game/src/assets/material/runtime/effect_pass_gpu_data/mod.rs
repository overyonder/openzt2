//! GPU register, texture dependency, and pipeline-key data for one effect pass.

mod material_configuration;
mod material_queries;
mod parameter_register_binding;
pub(super) mod uniform_buffer_upload;

use bevy::{
    asset::VisitAssetDependencies,
    prelude::*,
    render::render_resource::{
        AsBindGroup, BlendState, ColorWrites, CompareFunction, Face, PolygonMode, ShaderType,
    },
    render::storage::ShaderBuffer,
    shader::Shader,
};

use super::evaluated_d3d9_sampler_state::EvaluatedD3d9SamplerState;
use super::resolved_effect_transform_bindings::ResolvedEffectTransformSemantic;

pub(super) const FIXED_FUNCTION_SHADER: Handle<Shader> =
    bevy::asset::uuid_handle!("a4e864ef-5d33-46ab-a0ff-7d8549e09d0a");
pub(super) const EFFECT_SHADER_BINDING_LAYOUT: d3d9_effects::shader_types::D3d9ShaderBindingLayout =
    d3d9_effects::shader_types::D3d9ShaderBindingLayout {
        descriptor_set: 3,
        vertex_uniform_binding: 18,
        pixel_uniform_binding: 27,
        first_texture_binding: 2,
        texture_binding_count: 8,
        first_sampler_binding: 10,
        first_cube_texture_binding: 38,
    };
pub(crate) const FIXED_FUNCTION_WORLD_LIGHT_BUFFER: Handle<ShaderBuffer> =
    bevy::asset::uuid_handle!("3cc37db8-d774-40c4-80dc-790933570ae3");

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct D3d9FloatShaderRegisters {
    pub(super) values: [Vec4; 256],
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct D3d9IntegerShaderRegisters {
    pub(super) values: [IVec4; 16],
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct D3d9BooleanShaderRegisters {
    pub(super) values: [UVec4; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct D3d9FixedFunctionRegisters {
    pub(super) values: [Vec4; 48],
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct D3d9FixedFunctionTransforms {
    pub(super) values: [Mat4; 8],
}

pub(crate) const D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS: usize = 8;
pub(crate) const FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub(crate) struct D3d9FixedFunctionWorldLightRegisters {
    pub(crate) ambient_color_and_directional_light_count: Vec4,
    pub(crate) directional_light_directions: [Vec4; D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS],
    pub(crate) directional_light_colors: [Vec4; D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub(crate) struct D3d9FixedFunctionWorldLightContexts {
    pub(crate) values:
        [D3d9FixedFunctionWorldLightRegisters; FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT],
}

#[derive(AsBindGroup, Clone, Debug, VisitAssetDependencies)]
pub(crate) struct EffectPassTextureAssets {
    #[dependency]
    #[texture(2)]
    #[sampler(10)]
    texture0: Option<Handle<Image>>,
    #[dependency]
    #[texture(3)]
    #[sampler(11)]
    texture1: Option<Handle<Image>>,
    #[dependency]
    #[texture(4)]
    #[sampler(12)]
    texture2: Option<Handle<Image>>,
    #[dependency]
    #[texture(5)]
    #[sampler(13)]
    texture3: Option<Handle<Image>>,
    #[dependency]
    #[texture(6)]
    #[sampler(14)]
    texture4: Option<Handle<Image>>,
    #[dependency]
    #[texture(7)]
    #[sampler(15)]
    texture5: Option<Handle<Image>>,
    #[dependency]
    #[texture(8)]
    #[sampler(16)]
    texture6: Option<Handle<Image>>,
    #[dependency]
    #[texture(9)]
    #[sampler(17)]
    texture7: Option<Handle<Image>>,
    #[storage(37, read_only)]
    fixed_function_world_lights: Handle<ShaderBuffer>,
}

#[derive(Asset, TypePath, Clone, Debug)]
pub(crate) struct EffectPassMaterial {
    pub(super) texture_coordinate_animations:
        std::sync::Arc<[super::texture_coordinate_animation::TextureCoordinateAnimation]>,
    pub(super) vertex_uniform_upload_order: Option<std::sync::Arc<[Vec<u32>; 3]>>,
    pub(super) pixel_uniform_upload_order: Option<std::sync::Arc<[Vec<u32>; 3]>>,
    pub(super) lighting_state_is_authored: bool,
    pub(super) fixed_function: D3d9FixedFunctionRegisters,
    pub(super) fixed_transforms: D3d9FixedFunctionTransforms,
    pub(super) vertex_floats: D3d9FloatShaderRegisters,
    pub(super) vertex_integers: D3d9IntegerShaderRegisters,
    pub(super) vertex_booleans: D3d9BooleanShaderRegisters,
    pub(super) pixel_floats: D3d9FloatShaderRegisters,
    pub(super) pixel_integers: D3d9IntegerShaderRegisters,
    pub(super) pixel_booleans: D3d9BooleanShaderRegisters,
    #[dependency]
    pub(super) uniform_buffer_assets: EffectPassUniformBufferAssets,
    #[dependency]
    pub(super) texture_assets: EffectPassTextureAssets,
    pub(super) evaluated_sampler_states: [EvaluatedD3d9SamplerState; 8],
    pub(super) pipeline_specialization_key: EffectPassPipelineSpecializationKey,
    pub(super) dynamic_parameter_register_bindings:
        std::sync::Arc<[DynamicEffectParameterRegisterBinding]>,
    pub(super) parameter_input_dependencies: u8,
    pub(super) dirty_float_register_stages: u8,
    pub(super) dynamic_texture_semantics: [Option<String>; 8],
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct EffectPassUniformBufferAssets {
    #[dependency]
    pub(super) fixed_function: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) vertex_floats: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) vertex_integers: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) vertex_booleans: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) pixel_floats: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) pixel_integers: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) pixel_booleans: Handle<ShaderBuffer>,
    #[dependency]
    pub(super) fixed_transforms: Handle<ShaderBuffer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum D3d9ProgrammableShaderStage {
    Vertex,
    Pixel,
}

#[derive(Clone, Debug)]
pub(super) struct DynamicEffectParameterRegisterBinding {
    pub(super) semantic_name: String,
    pub(super) transform_semantic: ResolvedEffectTransformSemantic,
    pub(super) shader_stage: D3d9ProgrammableShaderStage,
    pub(super) register_set: u32,
    pub(super) first_register: u32,
    pub(super) register_count: u32,
    pub(super) is_row_major_matrix: bool,
}

pub(super) struct D3d9PackedShaderRegisters {
    values: [Vec4; 288],
    active_register_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EffectPassPipelineSpecializationKey {
    pub(super) vertex_shader: Handle<Shader>,
    pub(super) vertex_entry_point: &'static str,
    pub(super) vertex_inputs: Box<[d3d9_effects::shader_types::D3d9ShaderInputSignatureElement]>,
    pub(super) fragment_shader: Handle<Shader>,
    pub(super) fragment_entry_point: &'static str,
    pub(super) cull_face: Option<Face>,
    pub(super) polygon_mode: PolygonMode,
    pub(super) depth_write_enabled: bool,
    pub(super) depth_compare: CompareFunction,
    pub(super) blend_state: Option<BlendState>,
    pub(super) color_writes: ColorWrites,
}

impl From<&EffectPassMaterial> for EffectPassPipelineSpecializationKey {
    fn from(effect_pass_material: &EffectPassMaterial) -> Self {
        effect_pass_material.pipeline_specialization_key.clone()
    }
}
