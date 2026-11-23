//! Construction of Bevy material assets from evaluated D3D9 effect passes.

use std::io;

use bevy::{
    asset::LoadContext, image::ImageLoaderSettings, prelude::*, render::storage::ShaderBuffer,
    shader::Shader,
};

use super::{
    d3d9_shader_and_vertex_layout_mapping::map_d3d9_shader_input_to_bevy_vertex_attribute,
    effect_pass_gpu_data::{
        fixed_function_transform_binding::DynamicFixedFunctionTransformBinding,
        material_configuration::fixed_function_ambient_colour_from_argb,
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
    gpu_semantic_shader_registers::{
        reads_model, rewrite_translated_shader_with_gpu_semantic_registers, GpuSemanticRegisters,
        InstanceIndexInput, FORWARDED_INSTANCE_INDEX_LOCATION,
    },
    gpu_texture_transforms::{
        computed_on_gpu, default_inputs, fixed_function_source_with_texture_transforms,
        GpuTextureTransform, GPU_TRANSFORM_STAGE_REGISTER,
    },
    programmable_vertex_fixed_function_fragment_interface::create_fixed_function_fragment_shader_for_programmable_vertex_outputs,
    resolved_effect_transform_bindings::ResolvedEffectTransformSemantic,
};
use crate::material::authored_material_parameter_lowering::{
    MaterialEffectParameterValue, OwnedMaterialEffectParameter,
};

pub(in crate::material) fn create_effect_pass_material_assets_from_evaluated_technique(
    technique: &d3d9_effects::effect_types::EvaluatedD3d9EffectTechnique,
    parameter_descriptions: &[d3d9_effects::effect_types::D3d9EffectParameterDescription],
    parameters: &[OwnedMaterialEffectParameter],
    context: &mut LoadContext<'_>,
    labelled_asset_prefix: &str,
    texture_coordinate_animations: &std::sync::Arc<
        [super::texture_coordinate_animation::TextureCoordinateAnimation],
    >,
    reflection_projections: &[(usize, usize, [f32; 16])],
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
                reflection_projections,
                technique.evaluated_passes.len() == 1,
            )
        })
        .collect()
}

/// Uses the translated SPIR-V directly unless the shader computes renderer
/// semantics for itself.
fn translated_shader_asset(
    shader: d3d9_effects::shader_types::SpirvShaderTranslation,
    uniform_binding: u32,
    registers: &[GpuSemanticRegisters],
    instance_index: Option<InstanceIndexInput>,
    forward_instance_index: bool,
    sampler_biases: Option<&[f32; 8]>,
    path: String,
) -> io::Result<Shader> {
    let sampler_biases = sampler_biases.filter(|biases| biases.iter().any(|&bias| bias != 0.0));
    let fragment = uniform_binding == EFFECT_SHADER_BINDING_LAYOUT.pixel_uniform_binding;
    if !fragment && registers.is_empty() && !forward_instance_index && sampler_biases.is_none() {
        return Ok(Shader::from_spirv(
            shader.spirv_bytecode,
            format!("{path}.spv"),
        ));
    }
    let mut source = rewrite_translated_shader_with_gpu_semantic_registers(
        &shader.spirv_bytecode,
        uniform_binding,
        &shader.uniform_register_upload_order[0],
        registers,
        instance_index,
        forward_instance_index,
    )
    .map_err(|error| io::Error::new(error.kind(), format!("{path}: {error}")))?;
    if let Some(biases) = sampler_biases {
        super::gpu_semantic_shader_registers::apply_translated_sampler_biases(
            &mut source,
            EFFECT_SHADER_BINDING_LAYOUT,
            biases,
        )?;
    }
    if fragment {
        super::morph_fragment_opacity::apply_morph_opacity_to_translated_fragment(&mut source)?;
    }
    Ok(Shader::from_wesl(source, format!("{path}.wesl")))
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
    reflection_projections: &[(usize, usize, [f32; 16])],
    standalone_pass: bool,
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
    let gpu_registers = |stage| {
        dynamic_parameter_register_bindings
            .iter()
            .filter(|binding| {
                binding.shader_stage == stage
                    && binding.register_set == 2
                    && binding.transform_semantic.is_computed_on_gpu()
            })
            .map(|binding| GpuSemanticRegisters {
                semantic: binding.transform_semantic,
                first_register: binding.first_register,
                register_count: binding.register_count,
                is_row_major_matrix: binding.is_row_major_matrix,
            })
            .collect::<Vec<_>>()
    };
    let vertex_gpu_registers = gpu_registers(D3d9ProgrammableShaderStage::Vertex);
    let pixel_gpu_registers = gpu_registers(D3d9ProgrammableShaderStage::Pixel);
    let forward_instance_index = reads_model(&pixel_gpu_registers);
    // The renderer supplies these registers in the shader; owners bind the rest.
    let dynamic_parameter_register_bindings: std::sync::Arc<
        [DynamicEffectParameterRegisterBinding],
    > = dynamic_parameter_register_bindings
        .iter()
        .filter(|binding| {
            binding.register_set != 2 || !binding.transform_semantic.is_computed_on_gpu()
        })
        .cloned()
        .collect();
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
                format!("{}#{label}.wesl", context.path()),
            )
            .map(|shader| context.add_labeled_asset(label, shader))
        })
        .transpose()?;
    let vertex_entry = if vertex.is_some() { "main" } else { "vertex" };
    let programmable_vertex = vertex.is_some();
    let vertex_shader = match vertex {
        None => FIXED_FUNCTION_SHADER.clone(),
        Some(shader) => {
            let label = format!("{labelled_asset_prefix}-{index}-vertex");
            let asset = translated_shader_asset(
                shader,
                EFFECT_SHADER_BINDING_LAYOUT.vertex_uniform_binding,
                &vertex_gpu_registers,
                Some(InstanceIndexInput::VertexBuiltin),
                forward_instance_index,
                None,
                format!("{}#{label}", context.path()),
            )?;
            context.add_labeled_asset(label, asset)
        }
    };
    let programmable_fragment = fragment.is_some();
    let fragment_entry = if fragment.is_some() {
        "main"
    } else {
        "fragment"
    };
    let fragment_shader = match fragment {
        None => {
            fixed_fragment_for_programmable_vertex.unwrap_or_else(|| FIXED_FUNCTION_SHADER.clone())
        }
        Some(shader) => {
            let label = format!("{labelled_asset_prefix}-{index}-fragment");
            let asset = translated_shader_asset(
                shader,
                EFFECT_SHADER_BINDING_LAYOUT.pixel_uniform_binding,
                &pixel_gpu_registers,
                programmable_vertex.then_some(InstanceIndexInput::FlatLocation(
                    FORWARDED_INSTANCE_INDEX_LOCATION,
                )),
                false,
                Some(&state.samplers.map(|sampler| sampler.lod_bias())),
                format!("{}#{label}", context.path()),
            )?;
            context.add_labeled_asset(label, asset)
        }
    };
    let blend = state.blend()?;
    let (depth_write_enabled, depth_compare) = state.effective_depth_state();
    let (depth_bias_bits, slope_scale_depth_bias_bits) = state.effective_depth_bias_bits();
    let (stencil, stencil_reference) = state.effective_stencil();
    let independent_opaque = standalone_pass
        && blend.is_none()
        && depth_write_enabled
        && depth_compare == bevy::render::render_resource::CompareFunction::Greater
        && state.color_writes == bevy::render::render_resource::ColorWrites::ALL
        && stencil_reference.is_none();
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
    let mut fixed_function = D3d9FixedFunctionRegisters {
        values: std::array::from_fn(|index| match index {
            0..40 => {
                let texture_stage = index / 5;
                let first = (index % 5) * 4;
                let mut register = UVec4::from_array(std::array::from_fn(|component| {
                    state.stages[texture_stage]
                        .get(first + component)
                        .copied()
                        .unwrap_or_default()
                }))
                .as_vec4();
                if first == 16 {
                    register.z = state.samplers[texture_stage].lod_bias();
                }
                register
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
            47 => Vec4::new(
                0.0,
                f32::from(u8::from(state.ambient_state_is_authored)),
                0.0,
                0.0,
            ),
            48 => fixed_function_ambient_colour_from_argb(state.effect_states[49]),
            49 => UVec4::new(
                state.effect_states[15],
                state.effect_states[55],
                state.effect_states[52],
                0,
            )
            .as_vec4(),
            _ => unreachable!("fixed function register index is bounded by its array"),
        }),
    };
    let mut fixed_transform_bindings = state
        .texture_transform_expressions
        .iter()
        .enumerate()
        .filter_map(|(stage, expression)| expression.as_ref().map(|expression| (stage, expression)))
        .map(|(stage, expression)| {
            DynamicFixedFunctionTransformBinding::resolve(stage, expression, parameter_descriptions)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut model_reflection_projections = [None; 8];
    for &(pass, stage, matrix) in reflection_projections {
        if pass == index {
            let target = model_reflection_projections.get_mut(stage).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "reflection texture stage exceeds eight",
                )
            })?;
            *target = Some(Mat4::from_cols_array(&matrix));
        }
    }
    // Texture-stage transforms apply only where the fixed-function vertex and
    // fragment stages both run. Those that read renderer values run in the shader.
    let mut transform_inputs = Vec::new();
    let mut gpu_programs = Vec::new();
    if !programmable_vertex && !programmable_fragment {
        for binding in &mut fixed_transform_bindings {
            let semantics = binding.semantics();
            if !computed_on_gpu(&semantics) {
                continue;
            }
            let first_input = u32::try_from(transform_inputs.len()).map_err(io::Error::other)?;
            transform_inputs.extend(default_inputs(binding.program()).chunks(4).map(|chunk| {
                Vec4::from_array(std::array::from_fn(|component| {
                    chunk.get(component).copied().unwrap_or_default()
                }))
            }));
            binding.compute_on_gpu(first_input);
            gpu_programs.push((binding.stage(), semantics, first_input));
        }
    }
    let gpu_transforms = gpu_programs
        .iter()
        .filter_map(|(stage, dependencies, first_input)| {
            let binding = fixed_transform_bindings
                .iter()
                .find(|binding| binding.stage() == *stage)?;
            Some((
                *stage,
                GpuTextureTransform::Program {
                    program: binding.program(),
                    dependencies,
                    first_input: *first_input,
                },
            ))
        })
        .chain(
            model_reflection_projections
                .iter()
                .enumerate()
                .filter(|(stage, _)| {
                    !programmable_vertex
                        && !programmable_fragment
                        && !fixed_transform_bindings
                            .iter()
                            .any(|binding| binding.stage() == *stage)
                })
                .filter_map(|(stage, projection)| {
                    projection
                        .map(|projection| (stage, GpuTextureTransform::Reflection(projection)))
                }),
        )
        .collect::<Vec<_>>();
    // Stages the generated shader computes, which runtime reconfiguration can clear.
    fixed_function.values[GPU_TRANSFORM_STAGE_REGISTER].w = gpu_transforms
        .iter()
        .fold(0_u32, |mask, (stage, _)| mask | 1 << stage)
        as f32;
    let (vertex_shader, fragment_shader) = if gpu_transforms.is_empty() {
        (vertex_shader, fragment_shader)
    } else {
        let label = format!("{labelled_asset_prefix}-{index}-fixed-function");
        let source = fixed_function_source_with_texture_transforms(&gpu_transforms)
            .map_err(|error| io::Error::new(error.kind(), format!("{label}: {error}")))?;
        let shader = context.add_labeled_asset(
            label.clone(),
            Shader::from_wesl(
                super::morph_fragment_opacity::fixed_function_source_with_morph_opacity(&source),
                format!("{}#{label}.wesl", context.path()),
            ),
        );
        (shader.clone(), shader)
    };
    drop(gpu_transforms);
    if transform_inputs.is_empty() {
        transform_inputs.push(Vec4::ZERO);
    }
    let material = EffectPassMaterial {
        morph_opacity: Vec4::X,
        texture_coordinate_animation_geometry: None,
        texture_coordinate_animations: std::sync::Arc::clone(texture_coordinate_animations),
        lighting_state_is_authored: state.lighting_state_is_authored || material_lighting.is_some(),
        fixed_function,
        fixed_transforms: D3d9FixedFunctionTransforms {
            values: state.texture_transforms,
        },
        model_reflection_projections,
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
            transform_inputs: context.add_labeled_asset(
                format!("{labelled_asset_prefix}-{index}-transform-inputs"),
                ShaderBuffer::from(transform_inputs.clone()),
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
            depth_write_enabled,
            depth_compare,
            depth_bias_bits,
            slope_scale_depth_bias_bits,
            stencil,
            stencil_reference,
            independent_opaque,
            blend_state: blend,
            color_writes: state.color_writes,
        },
        dirty_float_register_stages: 0,
        fixed_transform_bindings,
        dirty_fixed_transforms: false,
        dirty_fixed_function: false,
        transform_inputs,
        dirty_transform_inputs: false,
        dynamic_parameter_register_bindings,
        dynamic_texture_semantics,
        vertex_uniform_upload_order,
        pixel_uniform_upload_order,
    };
    Ok(context.add_labeled_asset(format!("{labelled_asset_prefix}-{index}"), material))
}
