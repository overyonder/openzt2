//! Accumulated evaluated D3D9 state for one effect pass.

use std::io;

use bevy::{
    prelude::*,
    render::render_resource::{
        BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites, CompareFunction,
        Face, PolygonMode, StencilFaceState, StencilOperation, StencilState,
    },
};

use d3d9_effects::effect_evaluation::retained_transform_evaluation::RetainedD3d9TransformExpression;

use super::evaluated_d3d9_sampler_state::EvaluatedD3d9SamplerState;

pub(super) fn collect_evaluated_d3d9_pass_states(
    technique: &d3d9_effects::effect_types::EvaluatedD3d9EffectTechnique,
) -> io::Result<Vec<EvaluatedD3d9PassState>> {
    let mut state = EvaluatedD3d9PassState::default();
    technique
        .evaluated_passes
        .iter()
        .map(|pass| {
            pass.evaluated_commands
                .iter()
                .try_for_each(|command| state.apply(command))?;
            state.validate()?;
            Ok(state.clone())
        })
        .collect()
}

#[derive(Clone)]
pub(super) struct EvaluatedD3d9PassState {
    pub(super) vertex_shader: Option<Box<[u8]>>,
    pub(super) pixel_shader: Option<Box<[u8]>>,
    pub(super) vertex_floats: [Vec4; 256],
    pub(super) vertex_ints: [IVec4; 16],
    pub(super) vertex_bools: [UVec4; 4],
    pub(super) pixel_floats: [Vec4; 256],
    pub(super) pixel_ints: [IVec4; 16],
    pub(super) pixel_bools: [UVec4; 4],
    pub(super) textures: [Option<String>; 8],
    pub(super) samplers: [EvaluatedD3d9SamplerState; 8],
    pub(super) stages: [[u32; 18]; 8],
    pub(super) texture_transforms: [Mat4; 8],
    pub(super) texture_transform_expressions: [Option<RetainedD3d9TransformExpression>; 8],
    pub(super) material: [Vec4; 5],
    pub(super) effect_states: [u32; 103],
    pub(super) lighting_state_is_authored: bool,
    pub(super) ambient_state_is_authored: bool,
    pub(super) cull: Option<Face>,
    pub(super) polygon: PolygonMode,
    depth_enabled: bool,
    pub(super) depth_write: bool,
    depth_compare: CompareFunction,
    depth_bias_bits: u32,
    slope_scale_depth_bias_bits: u32,
    stencil_enabled: bool,
    stencil_two_sided: bool,
    stencil: StencilState,
    stencil_reference: u32,
    pub(super) blend_enabled: bool,
    pub(super) source_blend: u32,
    pub(super) destination_blend: u32,
    pub(super) blend_operation: u32,
    pub(super) separate_alpha_blend: bool,
    pub(super) source_alpha_blend: u32,
    pub(super) destination_alpha_blend: u32,
    pub(super) alpha_blend_operation: u32,
    pub(super) color_writes: ColorWrites,
}

impl Default for EvaluatedD3d9PassState {
    fn default() -> Self {
        let mut effect_states = [0; 103];
        // These are the D3D9 device defaults inherited by fixed-function
        // passes which do not author the state themselves. Blue Fang's Base
        // effects deliberately leave Lighting outside the material pass.
        effect_states[48] = 1; // D3DRS_LIGHTING = TRUE
        effect_states[11] = 8; // D3DRS_ALPHAFUNC = D3DCMP_ALWAYS
        effect_states[52] = 1; // D3DRS_LOCALVIEWER = TRUE
        effect_states[51] = 1; // D3DRS_COLORVERTEX = TRUE
        effect_states[54] = 1; // D3DRS_DIFFUSEMATERIALSOURCE = COLOR1
        effect_states[55] = 2; // D3DRS_SPECULARMATERIALSOURCE = COLOR2
        Self {
            vertex_shader: None,
            pixel_shader: None,
            vertex_floats: [Vec4::ZERO; 256],
            vertex_ints: [IVec4::ZERO; 16],
            vertex_bools: [UVec4::ZERO; 4],
            pixel_floats: [Vec4::ZERO; 256],
            pixel_ints: [IVec4::ZERO; 16],
            pixel_bools: [UVec4::ZERO; 4],
            textures: Default::default(),
            samplers: Default::default(),
            stages: std::array::from_fn(|stage| {
                let mut values = [0; 18];
                values[0] = if stage == 0 { 4 } else { 1 };
                values[1] = 2;
                values[2] = 1;
                values[3] = if stage == 0 { 4 } else { 1 };
                values[4] = 2;
                values[5] = 1;
                values
            }),
            texture_transforms: [Mat4::IDENTITY; 8],
            texture_transform_expressions: Default::default(),
            material: [Vec4::ZERO; 5],
            effect_states,
            lighting_state_is_authored: false,
            ambient_state_is_authored: false,
            cull: Some(Face::Back),
            polygon: PolygonMode::Fill,
            depth_enabled: true,
            depth_write: true,
            // Bevy's perspective projection uses reversed Z, so D3D9's
            // default LESS_EQUAL comparison becomes GREATER_EQUAL.
            depth_compare: CompareFunction::GreaterEqual,
            depth_bias_bits: 0,
            slope_scale_depth_bias_bits: 0,
            stencil_enabled: false,
            stencil_two_sided: false,
            stencil: StencilState {
                front: StencilFaceState::IGNORE,
                back: StencilFaceState::IGNORE,
                read_mask: u32::MAX,
                write_mask: u32::MAX,
            },
            stencil_reference: 0,
            blend_enabled: false,
            source_blend: 2,
            destination_blend: 1,
            blend_operation: 1,
            separate_alpha_blend: false,
            source_alpha_blend: 2,
            destination_alpha_blend: 1,
            alpha_blend_operation: 1,
            color_writes: ColorWrites::ALL,
        }
    }
}

impl EvaluatedD3d9PassState {
    pub(super) fn apply(
        &mut self,
        command: &d3d9_effects::effect_types::EvaluatedD3d9EffectCommand,
    ) -> io::Result<()> {
        use d3d9_effects::effect_types::EvaluatedD3d9EffectCommand;
        match command {
            EvaluatedD3d9EffectCommand::UnmappedD3d9EffectState {
                state_type,
                state_index,
                state_value,
            } => {
                if *state_type == 48 {
                    self.lighting_state_is_authored = true;
                }
                if *state_type == 49 {
                    self.ambient_state_is_authored = true;
                }
                if *state_index != 0 {
                    return Err(invalid("effect state index", *state_index));
                }
                *self
                    .effect_states
                    .get_mut(*state_type as usize)
                    .ok_or_else(|| invalid("effect state", *state_type))? = *state_value;
            }
            EvaluatedD3d9EffectCommand::D3d9RenderState {
                render_state,
                state_value,
            } => self.render_state(*render_state, *state_value)?,
            EvaluatedD3d9EffectCommand::D3d9TextureBinding {
                texture_stage,
                parameter_name,
            } => {
                if let Some(slot) = self.textures.get_mut(*texture_stage as usize) {
                    slot.clone_from(parameter_name);
                } else {
                    return Err(invalid("texture stage", *texture_stage));
                }
            }
            EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                texture_stage,
                texture_stage_state,
                state_value,
            } => {
                let slot = self
                    .stages
                    .get_mut(*texture_stage as usize)
                    .and_then(|states| {
                        texture_stage_state
                            .checked_sub(1)
                            .and_then(|state| states.get_mut(state as usize))
                    })
                    .ok_or_else(|| invalid("texture stage state", *texture_stage_state))?;
                *slot = *state_value;
            }
            EvaluatedD3d9EffectCommand::D3d9TransformState {
                transform_state,
                transform_matrix,
                retained_expression,
            } => {
                if let Some(stage) = transform_state.checked_sub(16) {
                    *self
                        .texture_transforms
                        .get_mut(stage as usize)
                        .ok_or_else(|| invalid("texture transform", stage))? =
                        Mat4::from_cols_array(transform_matrix);
                    self.texture_transform_expressions[stage as usize] =
                        retained_expression.clone();
                }
            }
            EvaluatedD3d9EffectCommand::D3d9MaterialState {
                material_state,
                material_values,
            } => {
                *self
                    .material
                    .get_mut(*material_state as usize)
                    .ok_or_else(|| invalid("material state", *material_state))? =
                    Vec4::from_array(*material_values);
            }
            EvaluatedD3d9EffectCommand::D3d9SamplerState {
                sampler_index,
                sampler_state,
                state_value,
            } => {
                self.samplers
                    .get_mut(*sampler_index as usize)
                    .ok_or_else(|| invalid("sampler", *sampler_index))?
                    .apply(*sampler_state, *state_value)?;
            }
            EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode { shader_bytecode } => {
                self.vertex_shader = (!shader_bytecode.is_empty()).then(|| shader_bytecode.clone());
            }
            EvaluatedD3d9EffectCommand::D3d9PixelShaderBytecode { shader_bytecode } => {
                self.pixel_shader = (!shader_bytecode.is_empty()).then(|| shader_bytecode.clone());
            }
            EvaluatedD3d9EffectCommand::D3d9VertexShaderFloatConstants {
                first_register,
                constant_values,
            } => {
                write_vec4(&mut self.vertex_floats, *first_register, constant_values)?;
            }
            EvaluatedD3d9EffectCommand::D3d9VertexShaderIntegerConstants {
                first_register,
                constant_values,
            } => {
                write_ivec4(&mut self.vertex_ints, *first_register, constant_values)?;
            }
            EvaluatedD3d9EffectCommand::D3d9VertexShaderBooleanConstants {
                first_register,
                constant_values,
            } => {
                write_bools(&mut self.vertex_bools, *first_register, constant_values)?;
            }
            EvaluatedD3d9EffectCommand::D3d9PixelShaderFloatConstants {
                first_register,
                constant_values,
            } => {
                write_vec4(&mut self.pixel_floats, *first_register, constant_values)?;
            }
            EvaluatedD3d9EffectCommand::D3d9PixelShaderIntegerConstants {
                first_register,
                constant_values,
            } => {
                write_ivec4(&mut self.pixel_ints, *first_register, constant_values)?;
            }
            EvaluatedD3d9EffectCommand::D3d9PixelShaderBooleanConstants {
                first_register,
                constant_values,
            } => {
                write_bools(&mut self.pixel_bools, *first_register, constant_values)?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported evaluated D3D9 command {other:?}"),
                ));
            }
        }
        Ok(())
    }
    pub(super) fn validate(&self) -> io::Result<()> {
        self.samplers
            .iter()
            .try_for_each(EvaluatedD3d9SamplerState::validate)?;
        if self.pixel_shader.is_none() {
            for stage in &self.stages {
                for (kind, value) in [("color operation", stage[0]), ("alpha operation", stage[3])]
                {
                    if !matches!(value, 1..=10 | 13..=18 | 24..=26) {
                        return Err(invalid(kind, value));
                    }
                }
                for value in [stage[1], stage[2], stage[4], stage[5]] {
                    if value & 0xf > 6 {
                        return Err(invalid("fixed-function argument", value));
                    }
                }
            }
        }
        Ok(())
    }
    fn render_state(&mut self, state: u32, value: u32) -> io::Result<()> {
        match state {
            7 => self.depth_enabled = value != 0,
            8 => {
                self.polygon = match value {
                    2 => PolygonMode::Line,
                    3 => PolygonMode::Fill,
                    _ => return Err(invalid("fill mode", value)),
                }
            }
            14 => self.depth_write = value != 0,
            19 => self.source_blend = value,
            20 => self.destination_blend = value,
            22 => {
                self.cull = match value {
                    1 => None,
                    2 => Some(Face::Back),
                    3 => Some(Face::Front),
                    _ => return Err(invalid("cull mode", value)),
                }
            }
            23 => self.depth_compare = compare(value)?,
            27 => self.blend_enabled = value != 0,
            52 => self.stencil_enabled = value != 0,
            53 => self.stencil.front.fail_op = stencil_operation(value)?,
            54 => self.stencil.front.depth_fail_op = stencil_operation(value)?,
            55 => self.stencil.front.pass_op = stencil_operation(value)?,
            56 => self.stencil.front.compare = compare_unreversed(value)?,
            57 => self.stencil_reference = value,
            58 => self.stencil.read_mask = value,
            59 => self.stencil.write_mask = value,
            139 => {
                self.effect_states[49] = value;
                self.ambient_state_is_authored = true;
            }
            168 => {
                self.color_writes = ColorWrites::from_bits(value)
                    .ok_or_else(|| invalid("color write mask", value))?;
            }
            171 => self.blend_operation = value,
            175 => self.slope_scale_depth_bias_bits = value,
            185 => self.stencil_two_sided = value != 0,
            186 => self.stencil.back.fail_op = stencil_operation(value)?,
            187 => self.stencil.back.depth_fail_op = stencil_operation(value)?,
            188 => self.stencil.back.pass_op = stencil_operation(value)?,
            189 => self.stencil.back.compare = compare_unreversed(value)?,
            195 => self.depth_bias_bits = value,
            206 => self.separate_alpha_blend = value != 0,
            207 => self.source_alpha_blend = value,
            208 => self.destination_alpha_blend = value,
            209 => self.alpha_blend_operation = value,
            _ => return Err(invalid("render state", state)),
        }
        Ok(())
    }

    pub(super) fn effective_depth_state(&self) -> (bool, CompareFunction) {
        if self.depth_enabled {
            (self.depth_write, self.depth_compare)
        } else {
            (false, CompareFunction::Always)
        }
    }

    pub(super) fn effective_depth_bias_bits(&self) -> (u32, u32) {
        if self.depth_enabled {
            (self.depth_bias_bits, self.slope_scale_depth_bias_bits)
        } else {
            (0, 0)
        }
    }

    pub(super) fn effective_stencil(&self) -> (StencilState, Option<u32>) {
        if !self.stencil_enabled {
            return (StencilState::default(), None);
        }
        let mut stencil = self.stencil.clone();
        if self.stencil_two_sided {
            // The D3D9 CW face maps to Bevy's back face in the cull-state lowering.
            std::mem::swap(&mut stencil.front, &mut stencil.back);
        } else {
            stencil.back = stencil.front;
        }
        stencil.read_mask &= 0xff;
        stencil.write_mask &= 0xff;
        if stencil.read_mask == 0 {
            // Both operands are zero. Keep the comparison active even with both masks zero.
            stencil.read_mask = 1;
            for face in [&mut stencil.front, &mut stencil.back] {
                face.compare = match face.compare {
                    CompareFunction::Always
                    | CompareFunction::Equal
                    | CompareFunction::LessEqual
                    | CompareFunction::GreaterEqual => CompareFunction::Always,
                    _ => CompareFunction::Never,
                };
            }
        }
        (stencil, Some(self.stencil_reference & 0xff))
    }

    pub(super) fn blend(&self) -> io::Result<Option<BlendState>> {
        self.blend_enabled
            .then(|| {
                Ok(BlendState {
                    color: BlendComponent {
                        src_factor: blend_factor(self.source_blend)?,
                        dst_factor: blend_factor(self.destination_blend)?,
                        operation: blend_operation(self.blend_operation)?,
                    },
                    alpha: BlendComponent {
                        src_factor: blend_factor(if self.separate_alpha_blend {
                            self.source_alpha_blend
                        } else {
                            self.source_blend
                        })?,
                        dst_factor: blend_factor(if self.separate_alpha_blend {
                            self.destination_alpha_blend
                        } else {
                            self.destination_blend
                        })?,
                        operation: blend_operation(if self.separate_alpha_blend {
                            self.alpha_blend_operation
                        } else {
                            self.blend_operation
                        })?,
                    },
                })
            })
            .transpose()
    }
}

fn compare(value: u32) -> io::Result<CompareFunction> {
    // D3D9 compares ordinary depth while Bevy compares reversed depth.
    Ok(match value {
        1 => CompareFunction::Never,
        2 => CompareFunction::Greater,
        3 => CompareFunction::Equal,
        4 => CompareFunction::GreaterEqual,
        5 => CompareFunction::Less,
        6 => CompareFunction::NotEqual,
        7 => CompareFunction::LessEqual,
        8 => CompareFunction::Always,
        _ => return Err(invalid("compare function", value)),
    })
}
pub(super) fn compare_unreversed(value: u32) -> io::Result<CompareFunction> {
    Ok(match value {
        1 => CompareFunction::Never,
        2 => CompareFunction::Less,
        3 => CompareFunction::Equal,
        4 => CompareFunction::LessEqual,
        5 => CompareFunction::Greater,
        6 => CompareFunction::NotEqual,
        7 => CompareFunction::GreaterEqual,
        8 => CompareFunction::Always,
        _ => return Err(invalid("compare function", value)),
    })
}

fn stencil_operation(value: u32) -> io::Result<StencilOperation> {
    Ok(match value {
        1 => StencilOperation::Keep,
        2 => StencilOperation::Zero,
        3 => StencilOperation::Replace,
        4 => StencilOperation::IncrementClamp,
        5 => StencilOperation::DecrementClamp,
        6 => StencilOperation::Invert,
        7 => StencilOperation::IncrementWrap,
        8 => StencilOperation::DecrementWrap,
        _ => return Err(invalid("stencil operation", value)),
    })
}

fn blend_factor(value: u32) -> io::Result<BlendFactor> {
    Ok(match value {
        1 => BlendFactor::Zero,
        2 => BlendFactor::One,
        3 => BlendFactor::Src,
        4 => BlendFactor::OneMinusSrc,
        5 => BlendFactor::SrcAlpha,
        6 => BlendFactor::OneMinusSrcAlpha,
        7 => BlendFactor::DstAlpha,
        8 => BlendFactor::OneMinusDstAlpha,
        9 => BlendFactor::Dst,
        10 => BlendFactor::OneMinusDst,
        11 => BlendFactor::SrcAlphaSaturated,
        14 => BlendFactor::Constant,
        15 => BlendFactor::OneMinusConstant,
        _ => return Err(invalid("blend factor", value)),
    })
}
fn blend_operation(value: u32) -> io::Result<BlendOperation> {
    Ok(match value {
        1 => BlendOperation::Add,
        2 => BlendOperation::Subtract,
        3 => BlendOperation::ReverseSubtract,
        4 => BlendOperation::Min,
        5 => BlendOperation::Max,
        _ => return Err(invalid("blend operation", value)),
    })
}
fn invalid(kind: &str, value: u32) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported D3D9 {kind} {value}"),
    )
}

fn write_vec4(target: &mut [Vec4; 256], register: u32, values: &[f32]) -> io::Result<()> {
    for (offset, value) in values.chunks(4).enumerate() {
        let array: [f32; 4] = value.try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported D3D9 float constant width {}", value.len()),
            )
        })?;
        let slot = target
            .get_mut(register as usize + offset)
            .ok_or_else(|| invalid("float register", register))?;
        *slot = Vec4::from_array(array);
    }
    Ok(())
}
fn write_ivec4(target: &mut [IVec4; 16], register: u32, values: &[i32]) -> io::Result<()> {
    for (offset, value) in values.chunks(4).enumerate() {
        let array: [i32; 4] = value.try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported D3D9 integer constant width {}", value.len()),
            )
        })?;
        *target
            .get_mut(register as usize + offset)
            .ok_or_else(|| invalid("integer register", register))? = IVec4::from_array(array);
    }
    Ok(())
}
fn write_bools(target: &mut [UVec4; 4], register: u32, values: &[bool]) -> io::Result<()> {
    for (offset, value) in values.iter().enumerate() {
        let index = register as usize + offset;
        let slot = target
            .get_mut(index / 4)
            .ok_or_else(|| invalid("boolean register", register))?;
        slot[index % 4] = u32::from(*value);
    }
    Ok(())
}
