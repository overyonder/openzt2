use super::{
    effect_compilation::compile_d3d9_effect_source_to_fx2_bytecode,
    effect_evaluation::compile_and_evaluate_d3d9_effect_source,
    effect_types::{
        D3d9EffectIncludeResolver, D3d9EffectParameterAssignment, D3d9EffectParameterValue,
        EvaluatedD3d9EffectCommand,
    },
    error::D3d9EffectProcessingError,
    shader_translation::assemble_d3d9_shader_assembly_source_to_bytecode,
};
use std::path::{Path, PathBuf};

struct NoIncludes;

#[test]
fn effect_shaders_do_not_reuse_literal_constant_registers() -> Result<(), D3d9EffectProcessingError>
{
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("literal-before-matrix.fx"),
        br"
        float4x4 projection;
        float4 literal(float4 p : POSITION) : POSITION { return p * 0.5; }
        float4 projected_position(float4 p : POSITION) : POSITION { return mul(p, projection); }
        technique T {
            pass A { VertexShader = compile vs_2_0 literal(); }
            pass B { VertexShader = compile vs_2_0 projected_position(); }
        }
    ",
        &mut NoIncludes,
        &[],
    )?;
    let bytecode = effect.evaluated_techniques[0].evaluated_passes[1]
        .evaluated_commands
        .iter()
        .find_map(|command| {
            if let EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode { shader_bytecode } =
                command
            {
                Some(shader_bytecode)
            } else {
                None
            }
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let shader = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        bytecode,
        &SHADER_BINDING_LAYOUT,
    )?;
    let matrix = shader
        .uniform_register_bindings
        .iter()
        .find(|binding| binding.parameter_name == "projection")
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    for register in matrix.first_register..matrix.first_register + matrix.register_count {
        assert!(
            shader.uniform_register_upload_order[0].contains(&register),
            "matrix register c{register} was replaced by a stale literal"
        );
    }
    Ok(())
}

#[test]
fn explicit_pixel_shader_constant_states_update_the_register_bank(
) -> Result<(), D3d9EffectProcessingError> {
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("coefficients.fx"),
        br"
        float4 coefficient = float4(0.25, 0.25, 1.0, 1.0);
        PixelShader shader = asm { ps_1_1 mov r0, c3 };
        technique T { pass P { PixelShader = shader; PixelShaderConstant1[3] = <coefficient>; } }
    ",
        &mut NoIncludes,
        &[],
    )?;
    let constants = effect.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .find_map(|command| {
            if let EvaluatedD3d9EffectCommand::D3d9PixelShaderFloatConstants {
                constant_values,
                ..
            } = command
            {
                Some(constant_values)
            } else {
                None
            }
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    assert_eq!(&constants[12..16], &[0.25, 0.25, 1.0, 1.0]);
    Ok(())
}

#[test]
fn reflects_sparse_uniform_upload_order() -> Result<(), D3d9EffectProcessingError> {
    let bytecode = assemble_d3d9_shader_assembly_source_to_bytecode(
        b"vs_2_0\ndcl_position v0\nmul r0, v0, c9\nadd oPos, r0, c3\n",
    )?;
    let shader = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        &bytecode,
        &SHADER_BINDING_LAYOUT,
    )?;
    assert_eq!(shader.uniform_register_upload_order[0], [3, 9]);
    assert!(shader.uniform_register_upload_order[1].is_empty());
    assert!(shader.uniform_register_upload_order[2].is_empty());
    Ok(())
}

#[test]
fn reflects_authored_matrix_register_packing() -> Result<(), D3d9EffectProcessingError> {
    for (qualifier, expected) in [("row_major", true), ("column_major", false)] {
        let source = format!("{qualifier} float4x4 transform; float4 main(float4 p : POSITION) : POSITION {{ return mul(p, transform); }} technique T {{ pass P {{ VertexShader = compile vs_2_0 main(); }} }}");
        let effect = compile_and_evaluate_d3d9_effect_source(
            Path::new("matrix.fx"),
            source.as_bytes(),
            &mut NoIncludes,
            &[],
        )?;
        let bytecode = effect.evaluated_techniques[0].evaluated_passes[0]
            .evaluated_commands
            .iter()
            .find_map(|command| {
                if let EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode { shader_bytecode } =
                    command
                {
                    Some(shader_bytecode)
                } else {
                    None
                }
            })
            .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        let shader = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
            bytecode,
            &SHADER_BINDING_LAYOUT,
        )?;
        let binding = shader
            .uniform_register_bindings
            .iter()
            .find(|binding| binding.parameter_name == "transform")
            .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        assert_eq!(binding.is_row_major_matrix, expected);
    }
    Ok(())
}

#[test]
fn effect_passes_allocate_nonoverlapping_uniform_registers() -> Result<(), D3d9EffectProcessingError>
{
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/shared-constants.fx"),
        br"
        float4x4 projection;
        float4 firstColour;
        float4 secondColour;
        float4 thirdColour;
        float4 first(float4 p : POSITION) : POSITION {
            return mul(p, projection) + firstColour;
        }
        float4 second(float4 p : POSITION) : POSITION {
            return mul(p, projection) + secondColour + thirdColour;
        }
        technique T {
            pass A { VertexShader = compile vs_2_0 first(); }
            pass B { VertexShader = compile vs_2_0 second(); }
        }",
        &mut NoIncludes,
        &[],
    )?;
    for pass in &effect.evaluated_techniques[0].evaluated_passes {
        for command in &pass.evaluated_commands {
            let EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode { shader_bytecode } = command
            else {
                continue;
            };
            let shader = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
                shader_bytecode,
                &SHADER_BINDING_LAYOUT,
            )?;
            let mut registers = std::collections::BTreeSet::new();
            for binding in &shader.uniform_register_bindings {
                for register in
                    binding.first_register..binding.first_register + binding.register_count
                {
                    assert!(
                        registers.insert((binding.register_set, register)),
                        "overlapping register for {}",
                        binding.parameter_name
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn declarationless_vertex_bytecode_returns_a_diagnostic_instead_of_crashing(
) -> Result<(), D3d9EffectProcessingError> {
    let bytecode = assemble_d3d9_shader_assembly_source_to_bytecode(
        b"vs_1_1\nmov oPos, v0\nmov oT0, v1\nmov oD0, v2\nmov oFog, c0.x\n",
    )?;
    let result = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        &bytecode,
        &SHADER_BINDING_LAYOUT,
    );
    assert!(
        matches!(result, Err(error) if error.to_string().contains("external vertex declarations"))
    );
    Ok(())
}

#[test]
fn links_standalone_vertex_outputs_for_fixed_function_fragments(
) -> Result<(), Box<dyn std::error::Error>> {
    let bytecode = assemble_d3d9_shader_assembly_source_to_bytecode(
        b"vs_2_0\ndcl_position v0\ndcl_texcoord v1\ndcl_color v2\nmov oPos, v0\nmov oT0, v1\nmov oD0, v2\nmov oFog, c0.x\n",
    )?;
    let shader = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        &bytecode,
        &SHADER_BINDING_LAYOUT,
    )?;
    let module = naga::front::spv::parse_u8_slice(
        &shader.spirv_bytecode,
        &naga::front::spv::Options::default(),
    )?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)?;
    assert_eq!(shader.vertex_output_signature_elements.len(), 3);
    // Legacy oFog is raster-output register 1 in MojoShader reflection.
    for (semantic, index, components) in [("TEXCOORD", 0, 4), ("COLOR", 0, 4), ("FOG", 1, 1)] {
        assert!(
            shader
                .vertex_output_signature_elements
                .iter()
                .any(|output| {
                    output.semantic_name == semantic
                        && output.semantic_index == index
                        && output.component_count == components
                }),
            "missing {semantic}: {:?}",
            shader.vertex_output_signature_elements
        );
    }
    let locations = shader
        .vertex_output_signature_elements
        .iter()
        .map(|output| output.location)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(locations, [0, 1, 2].into());
    Ok(())
}

#[test]
fn material_semantics_set_blending_vectors_and_direct_texture_bindings(
) -> Result<(), D3d9EffectProcessingError> {
    let source = br"
        bool alphaBlendEnable : AlphaBlendEnable = false;
        dword sourceBlend : SrcBlend = 1;
        dword destinationBlend : DestBlend = 1;
        float4 tint : MaterialTint = { 1.0, 1.0, 1.0, 1.0 };
        texture sampled : Texture0;
        technique T { pass P {
            AlphaBlendEnable = (alphaBlendEnable);
            SrcBlend = (sourceBlend);
            DestBlend = (destinationBlend);
            Texture[0] = (sampled);
            MaterialDiffuse = (tint);
        } }
    ";
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/material-semantics.fx"),
        source,
        &mut NoIncludes,
        &[
            D3d9EffectParameterAssignment {
                parameter_name: "MaterialTint",
                parameter_value: D3d9EffectParameterValue::FloatVector {
                    vector_components: &[0.2, 0.4, 0.6, 0.8],
                },
            },
            D3d9EffectParameterAssignment {
                parameter_name: "AlphaBlendEnable",
                parameter_value: D3d9EffectParameterValue::Boolean {
                    boolean_value: true,
                },
            },
            D3d9EffectParameterAssignment {
                parameter_name: "SrcBlend",
                parameter_value: D3d9EffectParameterValue::Integer { integer_value: 5 },
            },
            D3d9EffectParameterAssignment {
                parameter_name: "DestBlend",
                parameter_value: D3d9EffectParameterValue::Integer { integer_value: 6 },
            },
        ],
    )?;
    let commands = &effect.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
    assert!(commands.iter().any(|command| matches!(command,
        EvaluatedD3d9EffectCommand::D3d9MaterialState { material_state: 0, material_values }
            if material_values.map(f32::to_bits) == [0.2_f32, 0.4, 0.6, 0.8].map(f32::to_bits))));
    for (expected_state, expected_value) in [(27, 1), (19, 5), (20, 6)] {
        assert!(commands.iter().any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9RenderState { render_state, state_value }
                if *render_state == expected_state && *state_value == expected_value)));
    }
    assert!(commands.iter().any(|command| matches!(command,
        EvaluatedD3d9EffectCommand::D3d9TextureBinding { texture_stage: 0, parameter_name: Some(name) }
            if name == "sampled")));
    Ok(())
}

#[test]
#[ignore = "requires the local original-software archives"]
#[allow(
    clippy::too_many_lines,
    reason = "one corpus regression verifies compilation, state values, shader linking and wgpu validation together"
)]
fn evaluates_shipped_water_and_terrain_effect_states() -> Result<(), Box<dyn std::error::Error>> {
    struct ArchiveIncludes(z2f::ArchiveSet);
    impl D3d9EffectIncludeResolver for ArchiveIncludes {
        fn open(
            &mut self,
            parent: &Path,
            requested: &Path,
        ) -> Result<(PathBuf, Vec<u8>), D3d9EffectProcessingError> {
            let path = self
                .0
                .resolve_effect_include(parent, &requested.to_string_lossy())
                .ok_or_else(
                    || D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                        parent_effect_path: parent.to_owned(),
                        requested_include_path: requested.to_owned(),
                    },
                )?;
            let bytes = self.0.read(&path).map_err(|error| {
                D3d9EffectProcessingError::CompiledEffectEvaluationFailed(error.to_string())
            })?;
            Ok((path, bytes))
        }
    }
    let root = PathBuf::from(std::env::var("OPENZT2_Z2F_PATH")?);
    let mut paths = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "z2f"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut includes = ArchiveIncludes(z2f::ArchiveSet::open(paths)?);
    for name in [
        "effects/waterblit.fx",
        "effects/waterflat.fx",
        "effects/terrain.fx",
        "effects/base_wind.fx",
    ] {
        let path = Path::new(name);
        let bytes = includes.0.read(path)?;
        let (_, text) =
            z2f::text::decoding::decode_blue_fang_source_text_from_utf8_or_utf16_bytes(&bytes)?;
        let repaired =
            z2f::text::d3d9_effect_syntax::repair_observed_blue_fang_d3d9_effect_source_syntax(
                path, &text,
            );
        let evaluated =
            compile_and_evaluate_d3d9_effect_source(path, repaired.as_bytes(), &mut includes, &[])
                .map_err(|error| format!("{name} evaluation: {error}"))?;
        assert!(!evaluated.evaluated_techniques.is_empty());
        for technique in &evaluated.evaluated_techniques {
            for pass in &technique.evaluated_passes {
                let vertex = pass
                    .evaluated_commands
                    .iter()
                    .find_map(|command| match command {
                        EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode {
                            shader_bytecode,
                        } => Some(shader_bytecode),
                        _ => None,
                    });
                let pixel = pass
                    .evaluated_commands
                    .iter()
                    .find_map(|command| match command {
                        EvaluatedD3d9EffectCommand::D3d9PixelShaderBytecode { shader_bytecode } => {
                            Some(shader_bytecode)
                        }
                        _ => None,
                    });
                let shaders = match (vertex, pixel) {
                    (Some(vertex), Some(pixel)) => {
                        let (vertex, pixel) = super::shader_translation::translate_and_link_d3d9_vertex_and_pixel_shader_bytecode_to_spirv(vertex, pixel, &SHADER_BINDING_LAYOUT)?;
                        vec![vertex, pixel]
                    }
                    (Some(vertex), None) => vec![
                        super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
                            vertex,
                            &SHADER_BINDING_LAYOUT,
                        )?,
                    ],
                    _ => Vec::new(),
                };
                for shader in shaders {
                    let module = naga::front::spv::parse_u8_slice(
                        &shader.spirv_bytecode,
                        &naga::front::spv::Options::default(),
                    )
                    .map_err(|error| format!("{name}: {error}"))?;
                    naga::valid::Validator::new(
                        naga::valid::ValidationFlags::all(),
                        naga::valid::Capabilities::all(),
                    )
                    .validate(&module)
                    .map_err(|error| format!("{name}: {}", error.emit_to_string("")))?;
                    assert!(
                        !shader
                            .spirv_bytecode
                            .windows(4)
                            .any(|bytes| bytes == 0xdead_beef_u32.to_le_bytes()),
                        "{name}: unresolved shader interface location"
                    );
                }
                if name.ends_with("terrain.fx") {
                    assert!(pass.evaluated_commands.iter().any(|command| matches!(
                        command,
                        EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                            texture_stage: 0,
                            texture_stage_state: 2,
                            state_value: 2
                        }
                    )));
                    assert!(pass.evaluated_commands.iter().any(|command| matches!(
                        command,
                        EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                            texture_stage: 0,
                            texture_stage_state: 3,
                            state_value: 1
                        }
                    )));
                }
                for command in &pass.evaluated_commands {
                    if name.ends_with("terrain.fx") {
                        if let EvaluatedD3d9EffectCommand::D3d9MaterialState {
                            material_state: 0 | 1,
                            material_values,
                        } = command
                        {
                            assert_eq!(material_values.map(f32::to_bits), [1.0_f32.to_bits(); 4]);
                        }
                    }
                    if let EvaluatedD3d9EffectCommand::D3d9SamplerState {
                        sampler_state,
                        state_value,
                        ..
                    } = command
                    {
                        assert!((1..=13).contains(sampler_state));
                        if *sampler_state == 11 {
                            assert!(*state_value <= 1);
                        }
                    }
                }
                if name.ends_with("waterflat.fx") {
                    assert!(pass.evaluated_commands.iter().any(|command| matches!(command,
                        EvaluatedD3d9EffectCommand::D3d9TextureBinding { texture_stage: 0, parameter_name: Some(name) } if name == "tBumpMap")));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn assembles_legacy_shader_source() -> Result<(), D3d9EffectProcessingError> {
    let shader = assemble_d3d9_shader_assembly_source_to_bytecode(b"ps_1_1\ntex t0\nmov r0, t0\n")?;
    assert_eq!(&shader[..4], &0xffff_0101_u32.to_le_bytes());
    super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        &shader,
        &SHADER_BINDING_LAYOUT,
    )?;
    Ok(())
}

#[test]
fn compiles_inline_legacy_shader_with_assembly_comments() -> Result<(), D3d9EffectProcessingError> {
    let source = br#"
        PixelShader legacy = asm {
            ps_1_1
            ; Don't parse these as effect syntax: } { "
            tex t0
            mov r0, t0
        };
        technique T { pass P { PixelShader = legacy; } }
    "#;
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/inline.fx"),
        source,
        &mut NoIncludes,
        &[],
    )?;
    let shader = effect.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .find_map(|command| match command {
            EvaluatedD3d9EffectCommand::D3d9PixelShaderBytecode { shader_bytecode } => {
                Some(shader_bytecode)
            }
            _ => None,
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    assert_eq!(&shader[..4], &0xffff_0101_u32.to_le_bytes());
    Ok(())
}

#[test]
fn dynamic_float_inequality_preserves_unordered_operands() -> Result<(), D3d9EffectProcessingError>
{
    let source = br"
        float left;
        float right;
        technique T { pass P { AlphaBlendEnable = left != right; } }
    ";
    // Foreign compiler/evaluator semantics: explicit HLSL truth values, including
    // unordered operands, reach the FX_UINT state as normalized zero or one.
    for (left, right, expected_value) in [
        (0.75, 0.75, 0),
        (-0.5, 0.75, 1),
        (0.75, -0.5, 1),
        (0.0, -0.0, 0),
        (-0.0, 0.0, 0),
        (f32::INFINITY, f32::INFINITY, 0),
        (f32::NEG_INFINITY, f32::NEG_INFINITY, 0),
        (f32::INFINITY, f32::NEG_INFINITY, 1),
        (f32::NEG_INFINITY, f32::INFINITY, 1),
        (f32::INFINITY, 0.75, 1),
        (0.75, f32::NEG_INFINITY, 1),
        (f32::NAN, 0.75, 1),
        (0.75, f32::NAN, 1),
        (f32::NAN, f32::NAN, 1),
    ] {
        let effect = compile_and_evaluate_d3d9_effect_source(
            Path::new("effects/unordered-inequality.fx"),
            source,
            &mut NoIncludes,
            &[
                D3d9EffectParameterAssignment {
                    parameter_name: "left",
                    parameter_value: D3d9EffectParameterValue::FloatingPoint {
                        floating_point_value: left,
                    },
                },
                D3d9EffectParameterAssignment {
                    parameter_name: "right",
                    parameter_value: D3d9EffectParameterValue::FloatingPoint {
                        floating_point_value: right,
                    },
                },
            ],
        )?;
        let commands = &effect.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
        assert!(
            commands.iter().any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9RenderState { render_state: 27, state_value }
                if *state_value == expected_value)),
            "left={left:?}, right={right:?}, expected={expected_value}, commands={commands:?}"
        );
    }
    Ok(())
}

#[test]
fn evaluates_normalized_boolean_unsigned_casts() -> Result<(), D3d9EffectProcessingError> {
    let source = br"
        bool enabled = false;
        float density = 0.0;
        technique T { pass P {
            AlphaBlendEnable = enabled;
            SpecularEnable = enabled;
            FogEnable = enabled;
            LocalViewer = true;
            TexCoordIndex[0] = (uint)enabled * 3 + (uint)(density < -0.25) * 7 + 2;
        } }
    ";
    // Hand-derived arithmetic checks both parameter and comparison booleans.
    for (enabled, density, expected_coordinate_index) in
        [(true, -0.5, 12), (false, 0.5, 2), (true, 0.5, 5)]
    {
        let effect = compile_and_evaluate_d3d9_effect_source(
            Path::new("effects/boolean-casts.fx"),
            source,
            &mut NoIncludes,
            &[
                D3d9EffectParameterAssignment {
                    parameter_name: "enabled",
                    parameter_value: D3d9EffectParameterValue::Boolean {
                        boolean_value: enabled,
                    },
                },
                D3d9EffectParameterAssignment {
                    parameter_name: "density",
                    parameter_value: D3d9EffectParameterValue::FloatingPoint {
                        floating_point_value: density,
                    },
                },
            ],
        )?;
        let commands = &effect.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
        assert!(commands.iter().any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9RenderState { render_state: 27, state_value }
                if *state_value == u32::from(enabled))));
        for (expected_state, expected_value) in
            [(14, u32::from(enabled)), (15, u32::from(enabled)), (52, 1)]
        {
            assert!(commands.iter().any(|command| matches!(command,
                EvaluatedD3d9EffectCommand::UnmappedD3d9EffectState {
                    state_type, state_index: 0, state_value,
                } if *state_type == expected_state && *state_value == expected_value)));
        }
        assert!(commands.iter().any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                texture_stage: 0, texture_stage_state: 14, state_value,
            } if *state_value == expected_coordinate_index)));
    }
    Ok(())
}

#[test]
fn evaluates_dynamic_state_expressions() -> Result<(), D3d9EffectProcessingError> {
    let source = br"
        dword selector = 0;
        technique T { pass P {
            CullMode = selector ? CCW : CW;
            TexCoordIndex[0] = selector + 3;
        } }
    ";
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/dynamic.fx"),
        source,
        &mut NoIncludes,
        &[D3d9EffectParameterAssignment {
            parameter_name: "selector",
            parameter_value: D3d9EffectParameterValue::Integer { integer_value: 1 },
        }],
    )?;
    let commands = &effect.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
    assert!(
        commands.iter().any(|command| matches!(
            command,
            EvaluatedD3d9EffectCommand::D3d9RenderState {
                render_state: 22,
                state_value: 3
            }
        )),
        "{:?}",
        &commands[..2]
    );
    assert!(commands.iter().any(|command| matches!(
        command,
        EvaluatedD3d9EffectCommand::D3d9TextureStageState {
            texture_stage: 0,
            texture_stage_state: 14,
            state_value: 4
        }
    )));
    Ok(())
}

impl D3d9EffectIncludeResolver for NoIncludes {
    fn open(
        &mut self,
        parent: &Path,
        requested: &Path,
    ) -> Result<(PathBuf, Vec<u8>), D3d9EffectProcessingError> {
        Err(D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
            parent_effect_path: parent.to_owned(),
            requested_include_path: requested.to_owned(),
        })
    }
}

#[test]
fn compiles_fx2_state_and_shader_objects() -> Result<(), D3d9EffectProcessingError> {
    let source = br"
        float4 tint;
        float4 pixel() : COLOR { return tint; }
        technique arbitrary_name {
            pass arbitrary_pass {
                AlphaBlendEnable = true;
                PixelShader = compile ps_2_0 pixel();
            }
        }
    ";
    let effect = compile_d3d9_effect_source_to_fx2_bytecode(
        Path::new("effects/test.fx"),
        source,
        &mut NoIncludes,
    )?;
    assert!(!effect.compiled_effect_bytecode().is_empty());
    let evaluation = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/test.fx"),
        source,
        &mut NoIncludes,
        &[],
    )?;
    let technique = evaluation
        .evaluated_techniques
        .first()
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let pass = technique
        .evaluated_passes
        .first()
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    assert!(pass.evaluated_commands.iter().any(|command| matches!(
        command,
        EvaluatedD3d9EffectCommand::D3d9RenderState {
            render_state: 27,
            state_value: 1
        }
    )));
    assert!(pass.evaluated_commands.iter().any(|command| matches!(
        command,
        EvaluatedD3d9EffectCommand::D3d9PixelShaderBytecode { shader_bytecode }
            if !shader_bytecode.is_empty()
    )));
    Ok(())
}

#[test]
fn compiles_object_state_parameter_references() -> Result<(), D3d9EffectProcessingError> {
    let source = br"
        texture diffuse;
        technique T {
            pass P {
                Texture[2] = diffuse;
                ColorOp[2] = Modulate;
                AddressU[2] = Clamp;
            }
        }
    ";
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/object.fx"),
        source,
        &mut NoIncludes,
        &[],
    )?;
    let commands = &effect.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
    assert!(
        commands.iter().any(|command| matches!(
            command,
            EvaluatedD3d9EffectCommand::D3d9TextureBinding {
                texture_stage: 2,
                parameter_name: Some(parameter),
            }
                if parameter == "diffuse"
        )),
        "{commands:#?}"
    );
    assert!(commands.iter().any(|command| matches!(
        command,
        EvaluatedD3d9EffectCommand::D3d9TextureStageState {
            texture_stage: 2,
            texture_stage_state: 1,
            state_value: 4
        }
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        EvaluatedD3d9EffectCommand::D3d9SamplerState {
            sampler_index: 2,
            sampler_state: 1,
            state_value: 3
        }
    )));
    Ok(())
}

const SHADER_BINDING_LAYOUT: crate::shader_types::D3d9ShaderBindingLayout =
    crate::shader_types::D3d9ShaderBindingLayout {
        descriptor_set: 3,
        vertex_uniform_binding: 18,
        pixel_uniform_binding: 27,
        first_texture_binding: 2,
        texture_binding_count: 8,
        first_sampler_binding: 10,
        first_cube_texture_binding: 38,
    };

#[test]
fn evaluates_fractional_constants_in_dynamic_state_expressions(
) -> Result<(), D3d9EffectProcessingError> {
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/fog-density.fx"),
        b"float density = 0.0; technique T { pass P { FogDensity = density + 0.25; } }",
        &mut NoIncludes,
        &[D3d9EffectParameterAssignment {
            parameter_name: "density",
            parameter_value: D3d9EffectParameterValue::FloatingPoint {
                floating_point_value: 0.5,
            },
        }],
    )?;
    assert!(effect.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9RenderState { render_state: 38, state_value }
                if *state_value == 0.75_f32.to_bits())));
    Ok(())
}

#[test]
fn evaluates_negated_dynamic_state_parameters() -> Result<(), D3d9EffectProcessingError> {
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/negative-fog-density.fx"),
        b"float density = 0.0; technique T { pass P { FogDensity = -density; } }",
        &mut NoIncludes,
        &[D3d9EffectParameterAssignment {
            parameter_name: "density",
            parameter_value: D3d9EffectParameterValue::FloatingPoint {
                floating_point_value: 0.5,
            },
        }],
    )?;
    assert!(effect.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9RenderState { render_state: 38, state_value }
                if *state_value == (-0.5_f32).to_bits())));
    Ok(())
}

#[test]
fn retained_matrix_expression_preserves_swizzle_and_isolates_evaluation_inputs(
) -> Result<(), D3d9EffectProcessingError> {
    // Algorithm semantics and composition across the native compiler/evaluator:
    // exchange y/z in each basis row, discard translation, retain FX packed output.
    let first_input = [
        0.6, 0.8, 0.0, 0.0, -0.8, 0.6, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 6.0, 7.0, 1.0,
    ];
    let second_input = [
        0.36, -0.48, 0.8, 0.0, 0.8, 0.6, 0.0, 0.0, -0.48, 0.64, 0.6, 0.0, -19.0, 23.0, -29.0, 1.0,
    ];
    // Hand-derived rows are (x,z,y); twelve values are followed by four tail zeros.
    let first_expected = [
        0.6, 0.0, 0.8, -0.8, 0.0, 0.6, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let second_expected = [
        0.36, 0.8, -0.48, 0.8, 0.0, 0.6, -0.48, 0.6, 0.64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let effect = compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/basis-swizzle.fx"),
        br"
        float4x4 cameraBasis : InputView;
        technique T { pass P {
            TextureTransform[2] = float4x3(
                cameraBasis[0].xzy, cameraBasis[1].xzy,
                cameraBasis[2].xzy, float3(0.0, 0.0, 0.0));
        } }
        ",
        &mut NoIncludes,
        &[D3d9EffectParameterAssignment {
            parameter_name: "InputView",
            parameter_value: D3d9EffectParameterValue::FloatMatrix {
                matrix_components: &first_input,
            },
        }],
    )?;
    let (initial_matrix, expression) = effect.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .find_map(|command| {
            if let EvaluatedD3d9EffectCommand::D3d9TransformState {
                transform_matrix,
                retained_expression: Some(expression),
                ..
            } = command
            {
                Some((transform_matrix, expression))
            } else {
                None
            }
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    assert_eq!(*initial_matrix, first_expected);
    assert_eq!(
        expression.evaluate(&[D3d9EffectParameterAssignment {
            parameter_name: "InputView",
            parameter_value: D3d9EffectParameterValue::FloatMatrix {
                matrix_components: &first_input,
            },
        }])?,
        first_expected
    );
    assert_eq!(
        expression.evaluate(&[D3d9EffectParameterAssignment {
            parameter_name: "cameraBasis",
            parameter_value: D3d9EffectParameterValue::FloatMatrix {
                matrix_components: &second_input,
            },
        }])?,
        second_expected
    );
    let retained = expression.clone();
    drop(effect);
    assert_eq!(
        retained.evaluate(&[D3d9EffectParameterAssignment {
            parameter_name: "InputView",
            parameter_value: D3d9EffectParameterValue::FloatMatrix {
                matrix_components: &first_input,
            },
        }])?,
        first_expected
    );
    Ok(())
}

#[test]
fn dynamic_float_integer_round_trip_is_not_emitted_as_an_identity() {
    // Foreign compiler semantics: conversion of a fractional dynamic float to
    // int and back requires truncation. Reject it until FX emission implements it.
    let result = compile_d3d9_effect_source_to_fx2_bytecode(
        Path::new("effects/integer-density.fx"),
        b"float density; technique T { pass P { FogDensity = (float)(int)density; } }",
        &mut NoIncludes,
    );
    let error = result.expect_err("a value-changing dynamic cast must not compile as identity");
    assert!(matches!(
        error,
        D3d9EffectProcessingError::Vkd3dShaderEffectCompilationFailed(message)
            if message.contains("Value-changing numeric FX2 expression casts are not supported")
    ));
}

#[test]
fn direct_matrix_states_pack_logical_rows_in_initial_and_retained_evaluation(
) -> Result<(), D3d9EffectProcessingError> {
    // Algorithm semantics across the foreign compiler/evaluator: twelve logical
    // components survive, four unrelated row-padding sentinels never reach output.
    let first_input = [
        1.0, 2.0, 3.0, 91.0, 4.0, 5.0, 6.0, 92.0, 7.0, 8.0, 9.0, 93.0, 10.0, 11.0, 12.0, 94.0,
    ];
    let second_input = [
        -1.0, -2.0, -3.0, 81.0, -4.0, -5.0, -6.0, 82.0, -7.0, -8.0, -9.0, 83.0, -10.0, -11.0,
        -12.0, 84.0,
    ];
    let first_expected = [
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let second_expected = [
        -1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, 0.0, 0.0, 0.0,
        0.0,
    ];
    for source in [
        b"row_major float4x3 basis : InputBasis = {1,2,3,4,5,6,7,8,9,10,11,12}; technique T { pass P { TextureTransform[2] = basis; } }".as_slice(),
        b"column_major float4x3 basis : InputBasis = {1,2,3,4,5,6,7,8,9,10,11,12}; technique T { pass P { TextureTransform[2] = basis; } }".as_slice(),
    ] {
        // Default bytes independently establish the FX2 descriptor and row layout.
        let default_effect = compile_and_evaluate_d3d9_effect_source(
            Path::new("effects/direct-basis-default.fx"),
            source,
            &mut NoIncludes,
            &[],
        )?;
        let basis_parameter = default_effect
            .parameter_descriptions
            .iter()
            .find(|parameter| parameter.parameter_name == "basis")
            .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        assert_eq!((basis_parameter.row_count, basis_parameter.column_count), (4, 3));
        assert!(default_effect.evaluated_techniques[0].evaluated_passes[0]
            .evaluated_commands
            .iter()
            .any(|command| matches!(command,
                EvaluatedD3d9EffectCommand::D3d9TransformState { transform_matrix, .. }
                    if *transform_matrix == first_expected)));
        // Compile once per declaration majority; subsequent calls evaluate only.
        let effect = compile_and_evaluate_d3d9_effect_source(
            Path::new("effects/direct-basis.fx"),
            source,
            &mut NoIncludes,
            &[D3d9EffectParameterAssignment {
                parameter_name: "InputBasis",
                parameter_value: D3d9EffectParameterValue::FloatMatrix {
                    matrix_components: &first_input,
                },
            }],
        )?;
        let (initial, expression) = effect.evaluated_techniques[0].evaluated_passes[0]
            .evaluated_commands
            .iter()
            .find_map(|command| {
                if let EvaluatedD3d9EffectCommand::D3d9TransformState {
                    transform_matrix,
                    retained_expression: Some(expression),
                    ..
                } = command
                {
                    Some((transform_matrix, expression))
                } else {
                    None
                }
            })
            .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        let output_layout = expression.output_layout();
        assert_eq!(
            (output_layout.row_count, output_layout.column_count, output_layout.component_count),
            (3, 4, 12)
        );
        assert_eq!(*initial, first_expected);
        assert_eq!(
            expression.evaluate(&[D3d9EffectParameterAssignment {
                parameter_name: "basis",
                parameter_value: D3d9EffectParameterValue::FloatMatrix {
                    matrix_components: &second_input,
                },
            }])?,
            second_expected
        );
        let retained = expression.clone();
        drop(effect);
        assert_eq!(
            retained.evaluate(&[D3d9EffectParameterAssignment {
                parameter_name: "InputBasis",
                parameter_value: D3d9EffectParameterValue::FloatMatrix {
                    matrix_components: &first_input,
                },
            }])?,
            first_expected
        );
    }
    Ok(())
}

#[test]
fn struct_matrix_members_preserve_logical_rows_and_following_field_registers(
) -> Result<(), D3d9EffectProcessingError> {
    // Foreign compiler/data agreement and packing semantics: the declaration
    // independently specifies four logical rows, three columns, then a float4.
    // Compact defaults contain 16 floats; native assignments contain 20 floats.
    let first_input = [
        2.0, 4.0, 6.0, 91.0, 8.0, 10.0, 12.0, 92.0, 14.0, 16.0, 18.0, 93.0, 20.0, 22.0, 24.0, 94.0,
        211.0, 223.0, 227.0, 229.0,
    ];
    let second_input = [
        -3.0, -6.0, -9.0, 81.0, -12.0, -15.0, -18.0, 82.0, -21.0, -24.0, -27.0, 83.0, -30.0, -33.0,
        -36.0, 84.0, 311.0, 313.0, 317.0, 331.0,
    ];
    let row_major_expected = [
        &[
            1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0, 7.0, 8.0, 9.0, 0.0, 10.0, 11.0, 12.0, 0.0,
            101.0, 103.0, 107.0, 109.0,
        ][..],
        &[
            2.0, 4.0, 6.0, 0.0, 8.0, 10.0, 12.0, 0.0, 14.0, 16.0, 18.0, 0.0, 20.0, 22.0, 24.0, 0.0,
            211.0, 223.0, 227.0, 229.0,
        ][..],
        &[
            -3.0, -6.0, -9.0, 0.0, -12.0, -15.0, -18.0, 0.0, -21.0, -24.0, -27.0, 0.0, -30.0,
            -33.0, -36.0, 0.0, 311.0, 313.0, 317.0, 331.0,
        ][..],
    ];
    let column_major_expected = [
        &[
            1.0, 4.0, 7.0, 10.0, 2.0, 5.0, 8.0, 11.0, 3.0, 6.0, 9.0, 12.0, 101.0, 103.0, 107.0,
            109.0,
        ][..],
        &[
            2.0, 8.0, 14.0, 20.0, 4.0, 10.0, 16.0, 22.0, 6.0, 12.0, 18.0, 24.0, 211.0, 223.0,
            227.0, 229.0,
        ][..],
        &[
            -3.0, -12.0, -21.0, -30.0, -6.0, -15.0, -24.0, -33.0, -9.0, -18.0, -27.0, -36.0, 311.0,
            313.0, 317.0, 331.0,
        ][..],
    ];
    for (majority, full_register_count, full_expected, prefix_expected) in [
        (
            "row_major",
            5,
            row_major_expected,
            [
                &[1.0, 2.0, 3.0, 0.0][..],
                &[2.0, 4.0, 6.0, 0.0][..],
                &[-3.0, -6.0, -9.0, 0.0][..],
            ],
        ),
        (
            "column_major",
            4,
            column_major_expected,
            [
                &[1.0, 4.0, 7.0, 10.0][..],
                &[2.0, 8.0, 14.0, 20.0][..],
                &[-3.0, -12.0, -21.0, -30.0][..],
            ],
        ),
    ] {
        for (body, expected_register_count, expected, guard_declaration) in [
            (
                "return float4(mul(p, payload.basis), 1.0) + payload.following;",
                full_register_count,
                full_expected,
                "",
            ),
            (
                "return p * payload.basis[0][0] + guard;",
                1,
                prefix_expected,
                "float4 guard : register(c1) = {401,409,419,421};",
            ),
        ] {
            // Only the first component is used in the prefix shader. Every
            // reflected field remains present; c1 belongs to another uniform.
            let explicit_initializer = " = {{1,2,3,4,5,6,7,8,9,10,11,12}, {101,103,107,109}}";
            let absent_default_expected = [0.0; 20];
            for (initializer, assignment_name, input, expected_values) in [
                (explicit_initializer, "payload", None, expected[0]),
                (
                    "",
                    "payload",
                    None,
                    &absent_default_expected[..expected[0].len()],
                ),
                (
                    explicit_initializer,
                    "InputPayload",
                    Some(first_input.as_slice()),
                    expected[1],
                ),
                (
                    explicit_initializer,
                    "payload",
                    Some(second_input.as_slice()),
                    expected[2],
                ),
                (
                    explicit_initializer,
                    "payload",
                    Some(first_input.as_slice()),
                    expected[1],
                ),
            ] {
                let source = format!(
                    "struct Payload {{ {majority} float4x3 basis; float4 following; }};
                     Payload payload : InputPayload{initializer};
                     {guard_declaration}
                     float4 projected(float4 p : POSITION) : POSITION {{ {body} }}
                     technique T {{ pass P {{ VertexShader = compile vs_2_0 projected(); }} }}"
                );
                let assignments = input
                    .map(|aggregate_components| D3d9EffectParameterAssignment {
                        parameter_name: assignment_name,
                        parameter_value: D3d9EffectParameterValue::FloatAggregate {
                            aggregate_components,
                        },
                    })
                    .into_iter()
                    .collect::<Vec<_>>();
                let effect = compile_and_evaluate_d3d9_effect_source(
                    Path::new("effects/struct-basis.fx"),
                    source.as_bytes(),
                    &mut NoIncludes,
                    &assignments,
                )?;
                let parameter = effect
                    .parameter_descriptions
                    .iter()
                    .find(|parameter| parameter.parameter_name == "payload")
                    .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
                assert_eq!(parameter.parameter_class, 5);
                assert_eq!((parameter.row_count, parameter.column_count), (1, 20));
                assert_eq!(parameter.struct_members.len(), 2);
                let basis = &parameter.struct_members[0];
                assert_eq!(basis.member_name, "basis");
                assert_eq!((basis.parameter_class, basis.parameter_type), (2, 3));
                assert_eq!((basis.row_count, basis.column_count), (4, 3));
                assert_eq!(basis.array_element_count, 0);
                assert_eq!(basis.native_component_offset, 0);
                let following = &parameter.struct_members[1];
                assert_eq!(following.member_name, "following");
                assert_eq!(
                    (following.parameter_class, following.parameter_type),
                    (1, 3)
                );
                assert_eq!((following.row_count, following.column_count), (1, 4));
                assert_eq!(following.array_element_count, 0);
                assert_eq!(following.native_component_offset, 16);
                let first_register = assert_aggregate_vertex_register_values(
                    &effect,
                    "payload",
                    expected_register_count,
                    expected_values,
                )?;
                if !guard_declaration.is_empty() {
                    assert_eq!(first_register, 0);
                    assert_eq!(
                        assert_aggregate_vertex_register_values(
                            &effect,
                            "guard",
                            1,
                            &[401.0, 409.0, 419.0, 421.0],
                        )?,
                        1
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn partially_used_float_struct_keeps_unused_fields_out_of_neighboring_registers(
) -> Result<(), D3d9EffectProcessingError> {
    // Compiler/data agreement: both fields are reflected, only the first binds
    // c0. The separately authored c1 uniform must survive the aggregate upload.
    for (initializer, expected) in [
        (" = {{2,3,5,7}, {101,103,107,109}}", [2.0, 3.0, 5.0, 7.0]),
        ("", [0.0; 4]),
    ] {
        let source = format!(
            "struct Pair {{ float4 first; float4 unused; }};
             Pair payload{initializer};
             float4 guard : register(c1) = {{401,409,419,421}};
             float4 projected(float4 p : POSITION) : POSITION {{ return p + payload.first + guard; }}
             technique T {{ pass P {{ VertexShader = compile vs_2_0 projected(); }} }}"
        );
        let effect = compile_and_evaluate_d3d9_effect_source(
            Path::new("effects/partial-float-struct.fx"),
            source.as_bytes(),
            &mut NoIncludes,
            &[],
        )?;
        let payload = effect
            .parameter_descriptions
            .iter()
            .find(|parameter| parameter.parameter_name == "payload")
            .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        assert_eq!(payload.struct_members.len(), 2);
        assert_eq!((payload.row_count, payload.column_count), (1, 8));
        assert_eq!(payload.struct_members[0].member_name, "first");
        assert_eq!(payload.struct_members[1].member_name, "unused");
        assert_eq!(payload.struct_members[1].native_component_offset, 4);
        assert_eq!(
            assert_aggregate_vertex_register_values(&effect, "payload", 1, &expected)?,
            0
        );
        assert_eq!(
            assert_aggregate_vertex_register_values(
                &effect,
                "guard",
                1,
                &[401.0, 409.0, 419.0, 421.0],
            )?,
            1
        );
    }
    Ok(())
}

fn assert_aggregate_vertex_register_values(
    effect: &super::effect_types::EvaluatedD3d9Effect,
    parameter_name: &str,
    expected_register_count: u32,
    expected_values: &[f32],
) -> Result<u32, D3d9EffectProcessingError> {
    let commands = &effect.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
    let bytecode = commands
        .iter()
        .find_map(|command| match command {
            EvaluatedD3d9EffectCommand::D3d9VertexShaderBytecode { shader_bytecode } => {
                Some(shader_bytecode)
            }
            _ => None,
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let shader = super::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        bytecode,
        &SHADER_BINDING_LAYOUT,
    )?;
    let binding = shader
        .uniform_register_bindings
        .iter()
        .find(|binding| binding.parameter_name == parameter_name)
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    assert_eq!(binding.register_set, 2);
    assert_eq!(binding.register_count, expected_register_count);
    let constants = commands
        .iter()
        .find_map(|command| match command {
            EvaluatedD3d9EffectCommand::D3d9VertexShaderFloatConstants {
                first_register,
                constant_values,
            } if *first_register <= binding.first_register => {
                let start = usize::try_from((binding.first_register - first_register) * 4).ok()?;
                constant_values.get(start..start + expected_values.len())
            }
            _ => None,
        })
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    // Absent defaults and register padding require zero bits, including when
    // descriptor words would otherwise look like tiny subnormal floats.
    assert_eq!(
        constants
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        expected_values
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        "{parameter_name}"
    );
    Ok(binding.first_register)
}
