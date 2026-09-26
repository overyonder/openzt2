use std::path::{Path, PathBuf};

use bevy::render::render_resource::CompareFunction;

use super::effect_pass_gpu_data::EFFECT_SHADER_BINDING_LAYOUT;
use super::evaluated_d3d9_pass_state::collect_evaluated_d3d9_pass_states;

struct D3d9EffectIncludeResolver;

impl d3d9_effects::effect_types::D3d9EffectIncludeResolver for D3d9EffectIncludeResolver {
    fn open(
        &mut self,
        _: &Path,
        _: &Path,
    ) -> Result<(PathBuf, Vec<u8>), d3d9_effects::error::D3d9EffectProcessingError> {
        Ok((
            PathBuf::from("effects/value.fxh"),
            b"float4 Included = 1;".to_vec(),
        ))
    }
}

const EFFECT_SOURCE: &[u8] = br#"
#include "value.fxh"
bool WriteDepth = false;
float4 vertex_main(float4 position : POSITION) : POSITION { return position; }
float4 pixel_main() : COLOR { return Included; }
technique Novel {
  pass Fixed { ZWriteEnable = WriteDepth; ColorOp[0] = SelectArg1; ColorArg1[0] = Diffuse; }
  pass Programmed {
    VertexShader = compile vs_2_0 vertex_main();
    PixelShader = compile ps_2_0 pixel_main();
  }
}
"#;

const TWO_TEXTURE_FIXED_FUNCTION_EFFECT_SOURCE: &[u8] = br"
texture Texture0;
texture Texture1;
technique TAdvanced {
  pass P0 {
    Texture[0] = Texture0;
    Texture[1] = Texture1;
    ColorArg1[0] = Texture;
    ColorArg2[0] = Current;
    ColorOp[0] = Modulate;
    ColorArg1[1] = Texture;
    ColorArg2[1] = Current;
    ColorOp[1] = Add;
    ColorOp[2] = Disable;
    AlphaArg1[0] = Texture;
    AlphaArg2[0] = Current;
    AlphaOp[0] = Modulate;
    AlphaOp[1] = Disable;
    TexCoordIndex[0] = 0;
    TexCoordIndex[1] = 1;
  }
}
";

const FIXED_FUNCTION_LIGHTING_EFFECT_SOURCE: &[u8] = br"
float4 MaterialAmbientValue = { 0.25, 0.5, 0.75, 1.0 };
float4 MaterialDiffuseValue = { 0.75, 0.5, 0.25, 1.0 };
float4 MaterialEmissiveValue = { 0.1, 0.2, 0.3, 1.0 };
technique Lit {
  pass P0 {
    Lighting = true;
    MaterialAmbient = MaterialAmbientValue;
    MaterialDiffuse = MaterialDiffuseValue;
    MaterialEmissive = MaterialEmissiveValue;
    DiffuseMaterialSource = Material;
    AmbientMaterialSource = Color1;
    EmissiveMaterialSource = Material;
  }
}
";

#[test]
fn accumulates_parameter_state_and_programmable_shader_bytecode(
) -> Result<(), Box<dyn std::error::Error>> {
    let evaluate = |write_depth| {
        d3d9_effects::effect_evaluation::compile_and_evaluate_d3d9_effect_source(
            Path::new("effects/novel.fx"),
            EFFECT_SOURCE,
            &mut D3d9EffectIncludeResolver,
            &[d3d9_effects::effect_types::D3d9EffectParameterAssignment {
                parameter_name: "WriteDepth",
                parameter_value: d3d9_effects::effect_types::D3d9EffectParameterValue::Boolean {
                    boolean_value: write_depth,
                },
            }],
        )
    };
    let disabled = evaluate(false)?;
    let enabled = evaluate(true)?;
    let disabled_states = collect_evaluated_d3d9_pass_states(
        disabled
            .evaluated_techniques
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no disabled technique"))?,
    )?;
    let enabled_states = collect_evaluated_d3d9_pass_states(
        enabled
            .evaluated_techniques
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no enabled technique"))?,
    )?;
    assert!(
        !disabled_states
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no disabled pass"))?
            .depth_write
    );
    assert!(
        enabled_states
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no enabled pass"))?
            .depth_write
    );
    assert_eq!(
        enabled_states
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no enabled pass"))?
            .depth_compare,
        CompareFunction::GreaterEqual
    );
    let programmed = enabled_states
        .get(1)
        .ok_or_else(|| std::io::Error::other("D3DX returned no programmable pass"))?;
    let vertex = d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        programmed
            .vertex_shader
            .as_deref()
            .ok_or_else(|| std::io::Error::other("D3DX returned no vertex shader"))?,
        &EFFECT_SHADER_BINDING_LAYOUT,
    )
    .map_err(std::io::Error::other)?;
    let fragment = d3d9_effects::shader_translation::translate_d3d9_shader_bytecode_to_spirv(
        programmed
            .pixel_shader
            .as_deref()
            .ok_or_else(|| std::io::Error::other("D3DX returned no pixel shader"))?,
        &EFFECT_SHADER_BINDING_LAYOUT,
    )
    .map_err(std::io::Error::other)?;
    assert!(vertex.spirv_bytecode.starts_with(&[3, 2, 35, 7]));
    assert!(fragment.spirv_bytecode.starts_with(&[3, 2, 35, 7]));
    assert_eq!(
        vertex
            .input_signature_elements
            .first()
            .ok_or_else(|| std::io::Error::other("translated vertex shader has no input"))?
            .semantic_name,
        "POSITION"
    );
    Ok(())
}

#[test]
fn retains_two_texture_fixed_function_stage_bindings_and_compact_state_offsets(
) -> Result<(), Box<dyn std::error::Error>> {
    let effect = d3d9_effects::effect_evaluation::compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/two_texture.fx"),
        TWO_TEXTURE_FIXED_FUNCTION_EFFECT_SOURCE,
        &mut D3d9EffectIncludeResolver,
        &[],
    )?;
    let states = collect_evaluated_d3d9_pass_states(
        effect
            .evaluated_techniques
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no fixed-function technique"))?,
    )?;
    let state = states
        .first()
        .ok_or_else(|| std::io::Error::other("D3DX returned no fixed-function pass"))?;

    assert_eq!(state.textures[0].as_deref(), Some("Texture0"));
    assert!(!state.lighting_state_is_authored);
    assert_eq!(state.textures[1].as_deref(), Some("Texture1"));
    assert_eq!(&state.stages[0][..6], &[4, 2, 1, 4, 2, 1]);
    assert_eq!(&state.stages[1][..6], &[7, 2, 1, 1, 2, 1]);
    assert_eq!(state.stages[2][0], 1);
    assert_eq!(state.stages[0][13], 0);
    assert_eq!(state.stages[1][13], 1);
    Ok(())
}

#[test]
fn retains_fixed_function_lighting_materials_and_color_sources(
) -> Result<(), Box<dyn std::error::Error>> {
    let effect = d3d9_effects::effect_evaluation::compile_and_evaluate_d3d9_effect_source(
        Path::new("effects/lighting.fx"),
        FIXED_FUNCTION_LIGHTING_EFFECT_SOURCE,
        &mut D3d9EffectIncludeResolver,
        &[],
    )?;
    let states = collect_evaluated_d3d9_pass_states(
        effect
            .evaluated_techniques
            .first()
            .ok_or_else(|| std::io::Error::other("D3DX returned no lighting technique"))?,
    )?;
    let state = states
        .first()
        .ok_or_else(|| std::io::Error::other("D3DX returned no lighting pass"))?;

    assert_eq!(state.effect_states[48], 1);
    assert!(state.lighting_state_is_authored);
    assert_eq!(state.effect_states[54], 0);
    assert_eq!(state.effect_states[56], 1);
    assert_eq!(state.effect_states[57], 0);
    assert_eq!(
        state.material[0],
        bevy::prelude::Vec4::new(0.75, 0.5, 0.25, 1.0)
    );
    assert_eq!(
        state.material[1],
        bevy::prelude::Vec4::new(0.25, 0.5, 0.75, 1.0)
    );
    assert_eq!(
        state.material[3],
        bevy::prelude::Vec4::new(0.1, 0.2, 0.3, 1.0)
    );
    Ok(())
}
