use std::path::{Path, PathBuf};

use crate::error::D3d9EffectProcessingError;

#[derive(Clone, Debug)]
pub struct CompiledD3d9EffectBytecode(pub(crate) Box<[u8]>);

impl CompiledD3d9EffectBytecode {
    #[must_use]
    pub fn compiled_effect_bytecode(&self) -> &[u8] {
        &self.0
    }
}

pub trait D3d9EffectIncludeResolver {
    /// Resolves one include relative to the complete source file that requested it.
    ///
    /// # Errors
    ///
    /// Returns `D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved` when the requested source is unavailable.
    fn open(
        &mut self,
        parent_effect_path: &Path,
        requested_include_path: &Path,
    ) -> Result<(PathBuf, Vec<u8>), D3d9EffectProcessingError>;
}

#[derive(Clone, Debug)]
pub struct D3d9EffectParameterAssignment<'a> {
    pub parameter_name: &'a str,
    pub parameter_value: D3d9EffectParameterValue<'a>,
}

#[derive(Clone, Debug)]
pub enum D3d9EffectParameterValue<'a> {
    Boolean { boolean_value: bool },
    Integer { integer_value: i32 },
    FloatingPoint { floating_point_value: f32 },
    FloatVector { vector_components: &'a [f32] },
    FloatMatrix { matrix_components: &'a [f32] },
    TextureParameterReference { parameter_name: &'a str },
}

#[derive(Clone, Debug)]
pub struct EvaluatedD3d9Effect {
    pub parameter_descriptions: Box<[D3d9EffectParameterDescription]>,
    pub evaluated_techniques: Box<[EvaluatedD3d9EffectTechnique]>,
}

#[derive(Clone, Debug)]
pub struct D3d9EffectParameterDescription {
    pub parameter_name: String,
    pub semantic_name: Option<String>,
    pub parameter_class: u32,
    pub parameter_type: u32,
    pub row_count: u32,
    pub column_count: u32,
    pub array_element_count: u32,
    pub annotation_count: u32,
}

#[derive(Clone, Debug)]
pub struct EvaluatedD3d9EffectTechnique {
    pub technique_name: String,
    pub is_valid: bool,
    pub quality_annotation: Option<f32>,
    pub evaluated_passes: Box<[EvaluatedD3d9EffectPass]>,
}

#[derive(Clone, Debug)]
pub struct EvaluatedD3d9EffectPass {
    pub pass_name: String,
    pub evaluated_commands: Box<[EvaluatedD3d9EffectCommand]>,
}

#[derive(Clone, Debug)]
pub enum EvaluatedD3d9EffectCommand {
    UnmappedD3d9EffectState {
        state_type: u32,
        state_index: u32,
        state_value: u32,
    },
    D3d9TransformState {
        transform_state: u32,
        transform_matrix: [f32; 16],
    },
    D3d9MaterialState {
        material_state: u32,
        material_values: [f32; 4],
    },
    D3d9LightState {
        light_index: u32,
        light_data: Box<[u8]>,
    },
    D3d9LightEnabledState {
        light_index: u32,
        is_enabled: bool,
    },
    D3d9RenderState {
        render_state: u32,
        state_value: u32,
    },
    D3d9TextureBinding {
        texture_stage: u32,
        parameter_name: Option<String>,
    },
    D3d9TextureStageState {
        texture_stage: u32,
        texture_stage_state: u32,
        state_value: u32,
    },
    D3d9SamplerState {
        sampler_index: u32,
        sampler_state: u32,
        state_value: u32,
    },
    D3d9NPatchMode {
        segment_count: f32,
    },
    D3d9FlexibleVertexFormat {
        flexible_vertex_format_code: u32,
    },
    D3d9VertexShaderBytecode {
        shader_bytecode: Box<[u8]>,
    },
    D3d9VertexShaderFloatConstants {
        first_register: u32,
        constant_values: Box<[f32]>,
    },
    D3d9VertexShaderIntegerConstants {
        first_register: u32,
        constant_values: Box<[i32]>,
    },
    D3d9VertexShaderBooleanConstants {
        first_register: u32,
        constant_values: Box<[bool]>,
    },
    D3d9PixelShaderBytecode {
        shader_bytecode: Box<[u8]>,
    },
    D3d9PixelShaderFloatConstants {
        first_register: u32,
        constant_values: Box<[f32]>,
    },
    D3d9PixelShaderIntegerConstants {
        first_register: u32,
        constant_values: Box<[i32]>,
    },
    D3d9PixelShaderBooleanConstants {
        first_register: u32,
        constant_values: Box<[bool]>,
    },
}
