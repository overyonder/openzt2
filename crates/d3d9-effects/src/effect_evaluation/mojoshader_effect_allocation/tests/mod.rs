use std::path::{Path, PathBuf};

use crate::{
    effect_compilation::compile_d3d9_effect_source_to_fx2_bytecode,
    effect_evaluation::retained_transform_evaluation::RetainedD3d9EffectProgram,
    effect_types::{
        CompiledD3d9EffectBytecode, D3d9EffectIncludeResolver, EvaluatedD3d9EffectCommand,
    },
    error::D3d9EffectProcessingError,
};

use super::open_compiled_d3d9_effect_bytecode_with_mojoshader;

struct NoIncludes;

impl D3d9EffectIncludeResolver for NoIncludes {
    fn open(
        &mut self,
        parent_effect_path: &Path,
        requested_include_path: &Path,
    ) -> Result<(PathBuf, Vec<u8>), D3d9EffectProcessingError> {
        Err(D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
            parent_effect_path: parent_effect_path.to_owned(),
            requested_include_path: requested_include_path.to_owned(),
        })
    }
}

#[test]
fn foreign_expression_storage_errors_are_rejected_before_the_first_pass(
) -> Result<(), D3d9EffectProcessingError> {
    // Foreign-bytecode composition edge: the compiler's valid arithmetic gives
    // 0.5 + 0.25 = 0.75. Retargeting its final store outside the state allocation
    // or its literal outside CLIT must prevent an executable owner being returned.
    let compiled = compile_d3d9_effect_source_to_fx2_bytecode(
        Path::new("effects/execution-boundary.fx"),
        b"float density = 0.5; technique T { pass P { FogDensity = density + 0.25; } }",
        &mut NoIncludes,
    )?;
    let original = open_compiled_d3d9_effect_bytecode_with_mojoshader(&compiled)?;
    let evaluated = RetainedD3d9EffectProgram::new(original).copy_initial_evaluated_effect()?;
    assert!(evaluated.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .any(|command| matches!(command,
            EvaluatedD3d9EffectCommand::D3d9RenderState { render_state: 38, state_value }
                if *state_value == 0.75_f32.to_bits())));

    let mut words = compiled
        .compiled_effect_bytecode()
        .chunks_exact(4)
        .map(|bytes| {
            bytes
                .try_into()
                .map(u32::from_le_bytes)
                .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let fxlc = words
        .iter()
        .position(|word| *word == u32::from_le_bytes(*b"FXLC"))
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let clit = words
        .iter()
        .position(|word| *word == u32::from_le_bytes(*b"CLIT"))
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let mut cursor = fxlc + 2;
    let mut final_store = None;
    let mut literal_address = None;
    // FXLC encodes opcode/width, source count, then triples (array count,
    // storage kind, component address), with the destination last.
    for _ in 0..words[fxlc + 1] {
        let sources = usize::try_from(words[cursor + 1])
            .map_err(|_| D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
        for operand in 0..=sources {
            let triple = cursor + 2 + operand * 3;
            assert_eq!(words[triple], 0, "fixture has an indirect operand");
            if words[triple + 1] == 1 {
                literal_address = Some(triple + 2);
            }
            if operand == sources && words[triple + 1] == 4 {
                final_store = Some(triple + 2);
            }
        }
        cursor += 2 + (sources + 1) * 3;
    }
    let final_store =
        final_store.ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let literal_address = literal_address
        .ok_or(D3d9EffectProcessingError::NativeDependencyReturnedMalformedOutput)?;
    let original_store = words[final_store];
    words[final_store] = 16; // first component outside the evaluator's sixteen-float output
    assert!(matches!(
        open_compiled_d3d9_effect_bytecode_with_mojoshader(&words_to_bytecode(&words)),
        Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(reason))
            if reason.contains("output operand span")
    ));
    words[final_store] = original_store;
    words[literal_address] = words[clit + 1]; // one past the last CLIT literal
    assert!(matches!(
        open_compiled_d3d9_effect_bytecode_with_mojoshader(&words_to_bytecode(&words)),
        Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(reason))
            if reason.contains("could not parse effect state preshader")
    ));
    Ok(())
}

fn words_to_bytecode(words: &[u32]) -> CompiledD3d9EffectBytecode {
    CompiledD3d9EffectBytecode(words.iter().flat_map(|word| word.to_le_bytes()).collect())
}

#[test]
fn foreign_pass_sampler_references_require_sampler_parameter_storage(
) -> Result<(), D3d9EffectProcessingError> {
    // Foreign-bytecode agreement: this complete FX2 document names a sampler
    // with a diffuse texture and Clamp addressing. Only the type-1 reference
    // name changes in the mismatched case; both names resolve parsed parameters.
    let compiled = foreign_pass_sampler_reference_bytecode(Some(b"sampled\0"), 2);
    let allocation = open_compiled_d3d9_effect_bytecode_with_mojoshader(&compiled)?;
    let evaluated = RetainedD3d9EffectProgram::new(allocation).copy_initial_evaluated_effect()?;
    let commands = &evaluated.evaluated_techniques[0].evaluated_passes[0].evaluated_commands;
    assert!(commands.iter().any(|command| matches!(command,
        EvaluatedD3d9EffectCommand::D3d9TextureBinding {
            texture_stage: 2, parameter_name: Some(name),
        } if name == "diffuse")));
    assert!(commands.iter().any(|command| matches!(
        command,
        EvaluatedD3d9EffectCommand::D3d9SamplerState {
            sampler_index: 2,
            sampler_state: 1,
            state_value: 3,
        }
    )));

    for compiled in [
        foreign_pass_sampler_reference_bytecode(Some(b"numeric\0"), 2),
        foreign_pass_sampler_reference_bytecode(Some(b"sampled\0"), 0),
    ] {
        assert!(matches!(
            open_compiled_d3d9_effect_bytecode_with_mojoshader(&compiled),
            Err(D3d9EffectProcessingError::CompiledEffectEvaluationFailed(reason))
                if reason.contains("pass sampler parameter storage")
        ));
    }

    // An unused empty sampler and an unbound pass state reach no nested getters.
    let compiled = foreign_pass_sampler_reference_bytecode(None, 0);
    let allocation = open_compiled_d3d9_effect_bytecode_with_mojoshader(&compiled)?;
    let evaluated = RetainedD3d9EffectProgram::new(allocation).copy_initial_evaluated_effect()?;
    assert!(!evaluated.evaluated_techniques[0].evaluated_passes[0]
        .evaluated_commands
        .iter()
        .any(|command| matches!(
            command,
            EvaluatedD3d9EffectCommand::D3d9SamplerState { .. }
                | EvaluatedD3d9EffectCommand::D3d9TextureBinding { .. }
        )));
    Ok(())
}

fn foreign_pass_sampler_reference_bytecode(
    reference: Option<&[u8; 8]>,
    sampler_state_count: u32,
) -> CompiledD3d9EffectBytecode {
    // Offsets are bytes from the unstructured base, following the two-word
    // header. Objects 1 and 2 hold the texture mapping and pass carrier.
    let mut words = vec![0xfeff_0901, 51 * 4];
    words.extend_from_slice(&[
        0, // null string, word 0
        8,
        u32::from_le_bytes(*b"nume"),
        u32::from_le_bytes(*b"ric\0"),
        8,
        u32::from_le_bytes(*b"samp"),
        u32::from_le_bytes(*b"led\0"),
        2,
        u32::from_le_bytes(*b"T\0\0\0"),
        2,
        u32::from_le_bytes(*b"P\0\0\0"),
        8,
        u32::from_le_bytes(*b"diff"),
        u32::from_le_bytes(*b"use\0"),
    ]);
    words.extend_from_slice(&[3, 0, 4, 0, 1, 1, 1]); // float scalar descriptor, word 14
    words.push(0.5_f32.to_bits()); // numeric default, word 21
    words.extend_from_slice(&[12, 4, 16, 0, 1]); // sampler2D descriptor, word 22
    words.extend_from_slice(&[
        sampler_state_count, // sampler records, word 27
        4,
        0,
        36 * 4,
        41 * 4, // Texture = object 1
        5,
        0,
        42 * 4,
        49 * 4, // AddressU = Clamp
    ]);
    words.extend_from_slice(&[5, 4, 0, 0, 1]); // texture object descriptor, word 36
    words.push(1); // texture object index, word 41
    words.extend_from_slice(&[2, 0, 0, 0, 1, 1, 1]); // int scalar descriptor, word 42
    words.extend_from_slice(&[3, 2]); // Clamp and pass carrier object, words 49/50

    words.extend_from_slice(&[2, 1, 0, 3]); // parameter, technique and object counts
    words.extend_from_slice(&[14 * 4, 21 * 4, 0, 0]); // numeric parameter, no annotations
    words.extend_from_slice(&[22 * 4, 27 * 4, 0, 0]); // sampler parameter, no annotations
    words.extend_from_slice(&[7 * 4, 0, 1]); // technique T, one pass
    words.extend_from_slice(&[9 * 4, 0, 1]); // pass P, one state
    words.extend_from_slice(&[178, 2, 36 * 4, 50 * 4]); // Sampler[2], object carrier
    words.extend_from_slice(&[
        u32::from(sampler_state_count != 0),
        u32::from(reference.is_some()),
    ]); // small/large object counts
    if sampler_state_count != 0 {
        words.extend_from_slice(&[
            1,
            8,
            u32::from_le_bytes(*b"diff"),
            u32::from_le_bytes(*b"use\0"),
        ]); // texture mapping for object 1
    }
    if let Some(reference) = reference {
        words.extend_from_slice(&[0, 0, 0, 0, 1, 8]); // type-1 reference to pass state 0
        words.extend(
            reference
                .chunks_exact(4)
                .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        );
    }
    words_to_bytecode(&words)
}
