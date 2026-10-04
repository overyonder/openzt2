use crate::{
    effect_types::{EvaluatedD3d9EffectCommand, EvaluatedD3d9EffectPass},
    error::D3d9EffectProcessingError,
    native_abi::{
        openzt2_effect_pass_name, openzt2_effect_pixel_bool, openzt2_effect_pixel_float,
        openzt2_effect_pixel_int, openzt2_effect_pixel_shader, openzt2_effect_sampler_state_count,
        openzt2_effect_sampler_state_type, openzt2_effect_sampler_state_value,
        openzt2_effect_sampler_texture_mapping, openzt2_effect_state_count,
        openzt2_effect_state_float_values, openzt2_effect_state_index,
        openzt2_effect_state_mapping, openzt2_effect_state_type, openzt2_effect_state_value,
        openzt2_effect_vertex_bool, openzt2_effect_vertex_float, openzt2_effect_vertex_int,
        openzt2_effect_vertex_shader,
    },
};

use super::mojoshader_effect_allocation::{
    copy_mojoshader_retained_shader_bytecode, copy_optional_mojoshader_string,
    copy_required_mojoshader_string, MojoShaderEffectAllocationOwner,
};
use super::retained_transform_evaluation::RetainedD3d9EffectProgram;

#[allow(
    unsafe_code,
    reason = "copies one evaluated pass from the MojoShader ABI"
)]
pub(super) fn copy_evaluated_d3d9_effect_pass_from_mojoshader(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    retained_program: &RetainedD3d9EffectProgram,
    technique_index: u32,
    pass_index: u32,
) -> Result<EvaluatedD3d9EffectPass, D3d9EffectProcessingError> {
    let mut evaluated_commands = Vec::new();
    append_evaluated_effect_state_commands(
        effect_allocation,
        retained_program,
        technique_index,
        pass_index,
        &mut evaluated_commands,
    )?;
    append_evaluated_shader_bytecode_commands(effect_allocation, &mut evaluated_commands);
    append_evaluated_shader_constant_commands(effect_allocation, &mut evaluated_commands);
    Ok(EvaluatedD3d9EffectPass {
        pass_name: unsafe {
            copy_required_mojoshader_string(openzt2_effect_pass_name(
                effect_allocation.native_effect_pointer,
                technique_index,
                pass_index,
            ))?
        },
        evaluated_commands: evaluated_commands.into_boxed_slice(),
    })
}

#[allow(
    unsafe_code,
    reason = "copies evaluated pass states while their native allocation is alive"
)]
fn append_evaluated_effect_state_commands(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    retained_program: &RetainedD3d9EffectProgram,
    technique_index: u32,
    pass_index: u32,
    evaluated_commands: &mut Vec<EvaluatedD3d9EffectCommand>,
) -> Result<(), D3d9EffectProcessingError> {
    for effect_state_index in
        0..unsafe { openzt2_effect_state_count(effect_allocation.native_effect_pointer) }
    {
        let effect_state_type = unsafe {
            openzt2_effect_state_type(effect_allocation.native_effect_pointer, effect_state_index)
        };
        let effect_state_stage_or_index = unsafe {
            openzt2_effect_state_index(effect_allocation.native_effect_pointer, effect_state_index)
        };
        let effect_state_value = unsafe {
            openzt2_effect_state_value(effect_allocation.native_effect_pointer, effect_state_index)
        };
        if effect_state_type == 178 {
            append_evaluated_sampler_commands(
                effect_allocation,
                effect_state_index,
                effect_state_stage_or_index,
                evaluated_commands,
            );
        } else if effect_state_type == 164 {
            evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9TextureBinding {
                texture_stage: effect_state_stage_or_index,
                parameter_name: unsafe {
                    copy_optional_mojoshader_string(openzt2_effect_state_mapping(
                        effect_allocation.native_effect_pointer,
                        effect_state_index,
                    ))
                },
            });
        } else if matches!(effect_state_type, 123..=131) {
            let mut floating_point_state_values = [0.0; 16];
            unsafe {
                openzt2_effect_state_float_values(
                    effect_allocation.native_effect_pointer,
                    effect_state_index,
                    floating_point_state_values.as_mut_ptr(),
                    16,
                );
            }
            if (123..=126).contains(&effect_state_type) {
                evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9TransformState {
                    transform_state: if effect_state_type == 126 {
                        16 + effect_state_stage_or_index
                    } else {
                        effect_state_stage_or_index
                    },
                    transform_matrix: floating_point_state_values,
                    retained_expression: retained_program.retain_transform_expression(
                        effect_allocation,
                        technique_index,
                        pass_index,
                        effect_state_index,
                    )?,
                });
            } else {
                evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9MaterialState {
                    material_state: effect_state_type - 127,
                    material_values: floating_point_state_values[..4].try_into().map_err(|_| {
                        D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput
                    })?,
                });
            }
        } else if !matches!(effect_state_type, 146..=163) {
            evaluated_commands.push(convert_mojoshader_effect_state_to_evaluated_d3d9_command(
                effect_state_type,
                effect_state_stage_or_index,
                effect_state_value,
            ));
        }
    }
    Ok(())
}

#[allow(
    unsafe_code,
    reason = "copies sampler state owned by the live native effect"
)]
fn append_evaluated_sampler_commands(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    effect_state_index: u32,
    texture_stage: u32,
    evaluated_commands: &mut Vec<EvaluatedD3d9EffectCommand>,
) {
    evaluated_commands.extend(
        (0..unsafe {
            openzt2_effect_sampler_state_count(
                effect_allocation.native_effect_pointer,
                effect_state_index,
            )
        })
            .map(|sampler_state_index| {
                let state = unsafe {
                    openzt2_effect_sampler_state_type(
                        effect_allocation.native_effect_pointer,
                        effect_state_index,
                        sampler_state_index,
                    )
                };
                if state == 4 {
                    return EvaluatedD3d9EffectCommand::D3d9TextureBinding {
                        texture_stage,
                        parameter_name: unsafe {
                            copy_optional_mojoshader_string(openzt2_effect_sampler_texture_mapping(
                                effect_allocation.native_effect_pointer,
                                effect_state_index,
                                sampler_state_index,
                            ))
                        },
                    };
                }
                // MojoShader sampler-object enums start at AddressU=5;
                // D3DSAMPLERSTATETYPE starts at AddressU=1.
                EvaluatedD3d9EffectCommand::D3d9SamplerState {
                    sampler_index: texture_stage,
                    sampler_state: state.saturating_sub(4),
                    state_value: unsafe {
                        openzt2_effect_sampler_state_value(
                            effect_allocation.native_effect_pointer,
                            effect_state_index,
                            sampler_state_index,
                        )
                    },
                }
            }),
    );
}

#[allow(
    unsafe_code,
    reason = "copies bytecode owned by the live native effect"
)]
fn append_evaluated_shader_bytecode_commands(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    evaluated_commands: &mut Vec<EvaluatedD3d9EffectCommand>,
) {
    if let Some(vertex_shader_bytecode) = unsafe {
        copy_mojoshader_retained_shader_bytecode(
            effect_allocation.native_effect_pointer,
            openzt2_effect_vertex_shader,
        )
    } {
        evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode {
            shader_bytecode: vertex_shader_bytecode,
        });
    }
    if let Some(pixel_shader_bytecode) = unsafe {
        copy_mojoshader_retained_shader_bytecode(
            effect_allocation.native_effect_pointer,
            openzt2_effect_pixel_shader,
        )
    } {
        evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9PixelShaderBytecode {
            shader_bytecode: pixel_shader_bytecode,
        });
    }
}

#[allow(
    unsafe_code,
    reason = "copies the fixed-size constant banks owned by the live native effect"
)]
fn append_evaluated_shader_constant_commands(
    effect_allocation: &MojoShaderEffectAllocationOwner,
    evaluated_commands: &mut Vec<EvaluatedD3d9EffectCommand>,
) {
    evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9VertexShaderFloatConstants {
        first_register: 0,
        constant_values: unsafe {
            std::slice::from_raw_parts(
                openzt2_effect_vertex_float(effect_allocation.native_effect_pointer),
                1024,
            )
        }
        .into(),
    });
    evaluated_commands.push(
        EvaluatedD3d9EffectCommand::D3d9VertexShaderIntegerConstants {
            first_register: 0,
            constant_values: unsafe {
                std::slice::from_raw_parts(
                    openzt2_effect_vertex_int(effect_allocation.native_effect_pointer),
                    64,
                )
            }
            .into(),
        },
    );
    evaluated_commands.push(
        EvaluatedD3d9EffectCommand::D3d9VertexShaderBooleanConstants {
            first_register: 0,
            constant_values: unsafe {
                std::slice::from_raw_parts(
                    openzt2_effect_vertex_bool(effect_allocation.native_effect_pointer),
                    16,
                )
            }
            .iter()
            .map(|boolean_byte| *boolean_byte != 0)
            .collect(),
        },
    );
    evaluated_commands.push(EvaluatedD3d9EffectCommand::D3d9PixelShaderFloatConstants {
        first_register: 0,
        constant_values: unsafe {
            std::slice::from_raw_parts(
                openzt2_effect_pixel_float(effect_allocation.native_effect_pointer),
                1024,
            )
        }
        .into(),
    });
    evaluated_commands.push(
        EvaluatedD3d9EffectCommand::D3d9PixelShaderIntegerConstants {
            first_register: 0,
            constant_values: unsafe {
                std::slice::from_raw_parts(
                    openzt2_effect_pixel_int(effect_allocation.native_effect_pointer),
                    64,
                )
            }
            .into(),
        },
    );
    evaluated_commands.push(
        EvaluatedD3d9EffectCommand::D3d9PixelShaderBooleanConstants {
            first_register: 0,
            constant_values: unsafe {
                std::slice::from_raw_parts(
                    openzt2_effect_pixel_bool(effect_allocation.native_effect_pointer),
                    16,
                )
            }
            .iter()
            .map(|boolean_byte| *boolean_byte != 0)
            .collect(),
        },
    );
}

fn convert_mojoshader_effect_state_to_evaluated_d3d9_command(
    effect_state_type: u32,
    effect_state_index: u32,
    effect_state_value: u32,
) -> EvaluatedD3d9EffectCommand {
    if effect_state_type == 121 {
        return EvaluatedD3d9EffectCommand::D3d9NPatchMode {
            segment_count: f32::from_bits(effect_state_value),
        };
    }
    if effect_state_type == 122 {
        return EvaluatedD3d9EffectCommand::D3d9FlexibleVertexFormat {
            flexible_vertex_format_code: effect_state_value,
        };
    }
    if (103..=120).contains(&effect_state_type) {
        return EvaluatedD3d9EffectCommand::D3d9TextureStageState {
            texture_stage: effect_state_index,
            // The fixed-function carrier keeps Arg1/Arg2 beside each
            // operation. FX2 puts Arg0 before them; it is not that layout.
            texture_stage_state: match effect_state_type {
                104 => 7,
                105 => 2,
                106 => 3,
                107 => 4,
                108 => 8,
                109 => 5,
                110 => 6,
                _ => effect_state_type - 102,
            },
            state_value: effect_state_value,
        };
    }
    if (165..=176).contains(&effect_state_type) {
        return EvaluatedD3d9EffectCommand::D3d9SamplerState {
            sampler_index: effect_state_index,
            sampler_state: effect_state_type - 164,
            state_value: effect_state_value,
        };
    }
    let d3d9_render_state = match effect_state_type {
        0 => 7,
        1 => 8,
        3 => 14,
        6 => 19,
        7 => 20,
        8 => 22,
        9 => 23,
        13 => 27,
        20 => 38,
        73 => 168,
        75 => 171,
        99 => 206,
        100 => 207,
        101 => 208,
        102 => 209,
        _ => {
            return EvaluatedD3d9EffectCommand::UnmappedD3d9EffectState {
                state_type: effect_state_type,
                state_index: effect_state_index,
                state_value: effect_state_value,
            };
        }
    };
    EvaluatedD3d9EffectCommand::D3d9RenderState {
        render_state: d3d9_render_state,
        state_value: effect_state_value,
    }
}
