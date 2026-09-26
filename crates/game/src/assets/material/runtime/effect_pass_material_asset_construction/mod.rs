//! Construction of Bevy material assets from evaluated D3D9 effect passes.

use std::io;

use bevy::{asset::LoadContext, image::ImageLoaderSettings, prelude::*, shader::Shader};

use super::{
    d3d9_shader_and_vertex_layout_mapping::map_d3d9_shader_input_to_bevy_vertex_attribute,
    effect_pass_gpu_data::{
        uniform_buffer_upload::{
            pack_programmable_shader_registers, shader_buffer_from_packed_registers,
            shader_buffer_from_uniform_value,
        },
        D3d9BooleanShaderRegisters, D3d9FixedFunctionRegisters, D3d9FixedFunctionTransforms,
        D3d9FloatShaderRegisters, D3d9IntegerShaderRegisters, D3d9ProgrammableShaderStage,
        DynamicEffectParameterRegisterBinding, EffectPassMaterial,
        EffectPassPipelineSpecializationKey, EffectPassTextureAssets,
        EffectPassUniformBufferAssets, EFFECT_SHADER_BINDING_LAYOUT, FIXED_FUNCTION_SHADER,
    },
    evaluated_d3d9_pass_state::{collect_evaluated_d3d9_pass_states, EvaluatedD3d9PassState},
    programmable_vertex_fixed_function_fragment_interface::create_fixed_function_fragment_shader_for_programmable_vertex_outputs,
    resolved_effect_transform_bindings::ResolvedEffectTransformSemantic,
};
use crate::assets::material::authored_material_parameter_lowering::{
    MaterialEffectParameterValue, OwnedMaterialEffectParameter,
};

pub(in crate::assets::material) fn create_effect_pass_material_assets_from_evaluated_technique(
    technique: &d3d9_effects::effect_types::EvaluatedD3d9EffectTechnique,
    parameter_descriptions: &[d3d9_effects::effect_types::D3d9EffectParameterDescription],
    parameters: &[OwnedMaterialEffectParameter],
    context: &mut LoadContext<'_>,
    labelled_asset_prefix: &str,
    texture_coordinate_animations: &std::sync::Arc<
        [super::texture_coordinate_animation::TextureCoordinateAnimation],
    >,
) -> io::Result<Box<[Handle<EffectPassMaterial>]>> {
    collect_evaluated_d3d9_pass_states(technique)?
        .iter()
        .enumerate()
        .map(|(index, state)| {
            create_effect_pass_material_asset(
                state,
                parameter_descriptions,
                parameters,
                context,
                labelled_asset_prefix,
                index,
                texture_coordinate_animations,
            )
        })
        .collect()
}

#[allow(
    clippy::too_many_lines,
    reason = "pass construction follows the evaluated D3D state layout"
)]
fn create_effect_pass_material_asset(
    state: &EvaluatedD3d9PassState,
    parameter_descriptions: &[d3d9_effects::effect_types::D3d9EffectParameterDescription],
    parameters: &[OwnedMaterialEffectParameter],
    context: &mut LoadContext<'_>,
    labelled_asset_prefix: &str,
    index: usize,
    texture_coordinate_animations: &std::sync::Arc<
        [super::texture_coordinate_animation::TextureCoordinateAnimation],
    >,
) -> io::Result<Handle<EffectPassMaterial>> {
    let (vertex, fragment) = match (
        state.vertex_shader.as_deref(),
        state.pixel_shader.as_deref(),
    ) {
        (Some(vertex), Some(fragment)) => {
            let (vertex, fragment) =
                d3d9_effects::shader_translation::
                    translate_and_link_d3d9_vertex_and_pixel_shader_bytecode_to_spirv(
                        vertex, fragment, &EFFECT_SHADER_BINDING_LAYOUT,
                    )
                    .map_err(io::Error::other)?;
            (Some(vertex), Some(fragment))
        }
        (vertex, fragment) => (
            vertex
                .map(|bytecode| {
                    d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
                        bytecode,
                        &EFFECT_SHADER_BINDING_LAYOUT,
                    )
                })
                .transpose()
                .map_err(io::Error::other)?,
            fragment
                .map(|bytecode| {
                    d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
                        bytecode,
                        &EFFECT_SHADER_BINDING_LAYOUT,
                    )
                })
                .transpose()
                .map_err(io::Error::other)?,
        ),
    };
    let dynamic_parameter_register_bindings: std::sync::Arc<
        [DynamicEffectParameterRegisterBinding],
    > = [
        (D3d9ProgrammableShaderStage::Vertex, vertex.as_ref()),
        (D3d9ProgrammableShaderStage::Pixel, fragment.as_ref()),
    ]
    .into_iter()
    .flat_map(|(shader_stage, shader)| {
        shader.into_iter().flat_map(move |shader| {
            shader
                .uniform_register_bindings
                .iter()
                .filter_map(move |binding| {
                    parameter_descriptions
                        .iter()
                        .find(|parameter| {
                            parameter
                                .parameter_name
                                .eq_ignore_ascii_case(&binding.parameter_name)
                        })
                        .and_then(|parameter| parameter.semantic_name.as_ref())
                        .map(|semantic_name| DynamicEffectParameterRegisterBinding {
                            semantic_name: semantic_name.clone(),
                            transform_semantic: ResolvedEffectTransformSemantic::from_authored_name(
                                semantic_name,
                            ),
                            shader_stage,
                            register_set: binding.register_set,
                            first_register: binding.first_register,
                            register_count: binding.register_count,
                            is_row_major_matrix: binding.is_row_major_matrix,
                        })
                })
        })
    })
    .collect();
    if context.path().to_string().contains("waterflat") {
        bevy::log::debug!(target: "openzt2_effect_bindings", path = %context.path(), pass_index = index,
            bindings = ?dynamic_parameter_register_bindings,
            "loaded water shader semantic register bindings");
    }
    let vertex_uniform_upload_order = vertex
        .as_ref()
        .map(|shader| std::sync::Arc::new(shader.uniform_register_upload_order.clone()));
    let pixel_uniform_upload_order = fragment
        .as_ref()
        .map(|shader| std::sync::Arc::new(shader.uniform_register_upload_order.clone()));
    let vertex_inputs = vertex.as_ref().map_or_else(Box::default, |shader| {
        shader.input_signature_elements.clone().into_boxed_slice()
    });
    if let Some(input) = vertex_inputs
        .iter()
        .find(|input| map_d3d9_shader_input_to_bevy_vertex_attribute(input).is_none())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported vertex input {}{}",
                input.semantic_name, input.semantic_index
            ),
        ));
    }
    let fixed_fragment_for_programmable_vertex = vertex
        .as_ref()
        .filter(|_| fragment.is_none())
        .map(|vertex| {
            let label = format!("{labelled_asset_prefix}-{index}-fixed-fragment");
            create_fixed_function_fragment_shader_for_programmable_vertex_outputs(
                vertex,
                format!("{}#{label}", context.path()),
            )
            .map(|shader| context.add_labeled_asset(label, shader))
        })
        .transpose()?;
    let vertex_entry = if vertex.is_some() { "main" } else { "vertex" };
    let vertex_shader = vertex.map_or_else(
        || FIXED_FUNCTION_SHADER.clone(),
        |shader| {
            context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-vertex"),
                Shader::from_spirv(
                    shader.spirv_bytecode,
                    format!("{labelled_asset_prefix}-{index}-vertex.spv"),
                ),
            )
        },
    );
    let fragment_entry = if fragment.is_some() {
        "main"
    } else {
        "fragment"
    };
    let fragment_shader = fragment.map_or_else(
        || fixed_fragment_for_programmable_vertex.unwrap_or_else(|| FIXED_FUNCTION_SHADER.clone()),
        |shader| {
            context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-fragment"),
                Shader::from_spirv(
                    shader.spirv_bytecode,
                    format!("{labelled_asset_prefix}-{index}-fragment.spv"),
                ),
            )
        },
    );
    let blend = state.blend()?;
    let dynamic_texture_semantics = std::array::from_fn(|stage| {
        state.textures[stage].as_ref().and_then(|parameter_name| {
            parameter_descriptions
                .iter()
                .find(|description| {
                    description
                        .parameter_name
                        .eq_ignore_ascii_case(parameter_name)
                })
                .and_then(|description| description.semantic_name.clone())
        })
    });
    let mut textures: [Option<Handle<Image>>; 8] = Default::default();
    for (stage, parameter) in state.textures.iter().enumerate() {
        let Some(parameter) = parameter else {
            continue;
        };
        let semantic = parameter_descriptions
            .iter()
            .find(|description| description.parameter_name.eq_ignore_ascii_case(parameter))
            .and_then(|description| description.semantic_name.as_deref());
        let Some(candidate) = parameters.iter().find(|candidate| {
            candidate.parameter_name.eq_ignore_ascii_case(parameter)
                || semantic
                    .is_some_and(|semantic| candidate.parameter_name.eq_ignore_ascii_case(semantic))
        }) else {
            if parameter_descriptions.iter().any(|description| {
                description.parameter_name.eq_ignore_ascii_case(parameter)
                    && description.semantic_name.is_some()
            }) {
                continue;
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("D3DX bound unknown texture parameter {parameter}"),
            ));
        };
        let path = match &candidate.parameter_value {
            MaterialEffectParameterValue::TextureAssetPath(path) => Ok(path.as_deref()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("D3DX bound non-texture parameter {parameter} as a texture"),
            )),
        }?;
        let Some(path) = path else {
            continue;
        };
        let samples_texture_with_srgb_decode =
            state.samplers[stage].samples_texture_with_srgb_decode();
        textures[stage] = Some(
            context
                .load_builder()
                .with_settings(move |settings: &mut ImageLoaderSettings| {
                    settings.is_srgb = samples_texture_with_srgb_decode;
                })
                .load(path.to_string()),
        );
    }
    // Base.fx exposes this renderer-owned semantic but deliberately does not
    // assign the Lighting state inside its pass. Explicit FX states still win.
    let material_lighting = parameters.iter().find_map(|parameter| {
        if !parameter
            .parameter_name
            .eq_ignore_ascii_case("LightingEnable")
        {
            return None;
        }
        match parameter.parameter_value {
            MaterialEffectParameterValue::Boolean(enabled) => Some(enabled),
            MaterialEffectParameterValue::Integer(enabled) => Some(enabled != 0),
            _ => None,
        }
    });
    let fixed_function = D3d9FixedFunctionRegisters {
        values: std::array::from_fn(|index| match index {
            0..40 => {
                let texture_stage = index / 5;
                let first = (index % 5) * 4;
                UVec4::from_array(std::array::from_fn(|component| {
                    state.stages[texture_stage]
                        .get(first + component)
                        .copied()
                        .unwrap_or_default()
                }))
                .as_vec4()
            }
            40..45 => state.material[index - 40],
            45 => UVec4::new(
                state.effect_states[4],
                state.effect_states[10],
                state.effect_states[11],
                state.effect_states[14],
            )
            .as_vec4(),
            46 => UVec4::new(
                if state.lighting_state_is_authored {
                    state.effect_states[48]
                } else {
                    material_lighting.map_or(state.effect_states[48], u32::from)
                },
                state.effect_states[54],
                state.effect_states[56],
                state.effect_states[57],
            )
            .as_vec4(),
            _ => Vec4::ZERO,
        }),
    };
    let material = EffectPassMaterial {
        texture_coordinate_animations: std::sync::Arc::clone(texture_coordinate_animations),
        lighting_state_is_authored: state.lighting_state_is_authored || material_lighting.is_some(),
        fixed_function,
        fixed_transforms: D3d9FixedFunctionTransforms {
            values: state.texture_transforms,
        },
        vertex_floats: D3d9FloatShaderRegisters {
            values: state.vertex_floats,
        },
        vertex_integers: D3d9IntegerShaderRegisters {
            values: state.vertex_ints,
        },
        vertex_booleans: D3d9BooleanShaderRegisters {
            values: state.vertex_bools,
        },
        pixel_floats: D3d9FloatShaderRegisters {
            values: state.pixel_floats,
        },
        pixel_integers: D3d9IntegerShaderRegisters {
            values: state.pixel_ints,
        },
        pixel_booleans: D3d9BooleanShaderRegisters {
            values: state.pixel_bools,
        },
        uniform_buffer_assets: EffectPassUniformBufferAssets {
            fixed_function: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-fixed-function-uniform"),
                shader_buffer_from_uniform_value(&fixed_function),
            ),
            vertex_floats: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-vertex-float-uniform"),
                shader_buffer_from_packed_registers(&pack_programmable_shader_registers(
                    &D3d9FloatShaderRegisters {
                        values: state.vertex_floats,
                    },
                    &D3d9IntegerShaderRegisters {
                        values: state.vertex_ints,
                    },
                    &D3d9BooleanShaderRegisters {
                        values: state.vertex_bools,
                    },
                    vertex_uniform_upload_order.as_deref(),
                )),
            ),
            vertex_integers: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-vertex-integer-uniform"),
                shader_buffer_from_uniform_value(&D3d9IntegerShaderRegisters {
                    values: state.vertex_ints,
                }),
            ),
            vertex_booleans: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-vertex-boolean-uniform"),
                shader_buffer_from_uniform_value(&D3d9BooleanShaderRegisters {
                    values: state.vertex_bools,
                }),
            ),
            pixel_floats: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-pixel-float-uniform"),
                shader_buffer_from_packed_registers(&pack_programmable_shader_registers(
                    &D3d9FloatShaderRegisters {
                        values: state.pixel_floats,
                    },
                    &D3d9IntegerShaderRegisters {
                        values: state.pixel_ints,
                    },
                    &D3d9BooleanShaderRegisters {
                        values: state.pixel_bools,
                    },
                    pixel_uniform_upload_order.as_deref(),
                )),
            ),
            pixel_integers: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-pixel-integer-uniform"),
                shader_buffer_from_uniform_value(&D3d9IntegerShaderRegisters {
                    values: state.pixel_ints,
                }),
            ),
            pixel_booleans: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-pixel-boolean-uniform"),
                shader_buffer_from_uniform_value(&D3d9BooleanShaderRegisters {
                    values: state.pixel_bools,
                }),
            ),
            fixed_transforms: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-fixed-transform-uniform"),
                shader_buffer_from_uniform_value(&D3d9FixedFunctionTransforms {
                    values: state.texture_transforms,
                }),
            ),
        },
        texture_assets: EffectPassTextureAssets::from_ordered_texture_assets(textures),
        evaluated_sampler_states: state.samplers,
        pipeline_specialization_key: EffectPassPipelineSpecializationKey {
            vertex_shader,
            vertex_entry_point: vertex_entry,
            vertex_inputs,
            fragment_shader,
            fragment_entry_point: fragment_entry,
            cull_face: state.cull,
            polygon_mode: state.polygon,
            depth_write_enabled: state.depth_write,
            depth_compare: state.depth_compare,
            blend_state: blend,
            color_writes: state.color_writes,
        },
        parameter_input_dependencies: dynamic_parameter_register_bindings
            .iter()
            .fold(0, |mask, binding| {
                mask | binding.transform_semantic.input_dependencies()
            }),
        dirty_float_register_stages: 0,
        dynamic_parameter_register_bindings,
        dynamic_texture_semantics,
        vertex_uniform_upload_order,
        pixel_uniform_upload_order,
    };
    Ok(context.add_labeled_asset(format!("{labelled_asset_prefix}-{index}"), material))
}
