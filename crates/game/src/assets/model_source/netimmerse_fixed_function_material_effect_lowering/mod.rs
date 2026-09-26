//! NetImmerse material properties lowered into the existing D3D9 pass owner.

use std::io;

use d3d9_effects::effect_types::{
    EvaluatedD3d9Effect, EvaluatedD3d9EffectCommand, EvaluatedD3d9EffectPass,
    EvaluatedD3d9EffectTechnique,
};

use super::model::NativeMaterialSource;

pub(super) struct LoweredNetImmerseFixedFunctionMaterial {
    pub(super) evaluated_effect: EvaluatedD3d9Effect,
    pub(super) texture_asset_paths: Vec<(String, String)>,
}

pub(super) fn lower_netimmerse_material_into_evaluated_d3d9_effect(
    material: &NativeMaterialSource,
) -> io::Result<LoweredNetImmerseFixedFunctionMaterial> {
    let mut commands = vec![
        material_state(0, material.base_color),
        material_state(
            1,
            [
                material.ambient[0],
                material.ambient[1],
                material.ambient[2],
                material.base_color[3],
            ],
        ),
        material_state(
            2,
            [
                material.specular[0],
                material.specular[1],
                material.specular[2],
                material.base_color[3],
            ],
        ),
        material_state(
            3,
            [
                material.emissive[0],
                material.emissive[1],
                material.emissive[2],
                material.base_color[3],
            ],
        ),
        material_state(4, [material.glossiness, 0.0, 0.0, 0.0]),
        effect_state(48, 1),
    ];

    if material.lighting_mode > 1 {
        return Err(invalid_native_material_state(
            "vertex lighting mode",
            material.lighting_mode,
        ));
    }
    let (mut diffuse_source, mut ambient_source, emissive_source) = match material.vertex_color_mode
    {
        0 => (0, 0, 0),
        1 => (0, 0, 1),
        2 => (1, 1, 0),
        mode => return Err(invalid_native_material_state("vertex color mode", mode)),
    };
    if material.lighting_mode == 0 {
        diffuse_source = 0;
        ambient_source = 0;
    }
    commands.extend([
        effect_state(54, diffuse_source),
        effect_state(56, ambient_source),
        effect_state(57, emissive_source),
    ]);
    if material.lighting_mode == 0 {
        commands.extend([
            material_state(0, [0.0, 0.0, 0.0, material.base_color[3]]),
            material_state(1, [0.0, 0.0, 0.0, material.base_color[3]]),
        ]);
    }

    let mut texture_asset_paths = Vec::new();
    if let Some(base) = &material.base_texture {
        let parameter = "NetImmerseBaseTexture".to_owned();
        texture_asset_paths.push((parameter.clone(), base.asset_path.clone()));
        commands.push(texture_binding(0, parameter));
        commands.push(texture_stage_state(0, 14, base.uv_set));
        commands.extend(netimmerse_sampler_states(0, base)?);
        match material.texture_apply_mode {
            // APPLY_REPLACE
            0 => commands.extend([
                texture_stage_state(0, 1, 2),
                texture_stage_state(0, 2, 2),
                texture_stage_state(0, 4, 2),
                texture_stage_state(0, 5, 2),
            ]),
            // APPLY_DECAL: blend the texture RGB over the lit colour with
            // the texture alpha while preserving the material alpha.
            1 => commands.extend([
                texture_stage_state(0, 1, 13),
                texture_stage_state(0, 2, 2),
                texture_stage_state(0, 3, 1),
                texture_stage_state(0, 4, 2),
                texture_stage_state(0, 5, 1),
            ]),
            // APPLY_MODULATE uses the D3D device default already represented
            // by the fixed-function pass owner.
            2 => {}
            mode => return Err(invalid_native_material_state("texture apply mode", mode)),
        }
    }
    if let Some(detail) = &material.detail_texture {
        let parameter = "NetImmerseDetailTexture".to_owned();
        texture_asset_paths.push((parameter.clone(), detail.asset_path.clone()));
        commands.extend([
            texture_binding(1, parameter),
            // Gamebryo detail maps are centred around half intensity and use
            // D3DTOP_MODULATE2X so neutral grey leaves the base map unchanged.
            texture_stage_state(1, 1, 5),
            texture_stage_state(1, 2, 2),
            texture_stage_state(1, 3, 1),
            texture_stage_state(1, 4, 2),
            texture_stage_state(1, 5, 1),
            texture_stage_state(1, 14, detail.uv_set),
        ]);
        commands.extend(netimmerse_sampler_states(1, detail)?);
    }
    if let Some(glow) = &material.glow_texture {
        let stage = if material.detail_texture.is_some() {
            2
        } else {
            1
        };
        let parameter = "NetImmerseGlowTexture".to_owned();
        texture_asset_paths.push((parameter.clone(), glow.asset_path.clone()));
        commands.extend([
            texture_binding(stage, parameter),
            // Glow is an unlit additive contribution after the lit base
            // and detail maps. It does not replace the surface opacity.
            texture_stage_state(stage, 1, 7),
            texture_stage_state(stage, 2, 2),
            texture_stage_state(stage, 3, 1),
            texture_stage_state(stage, 4, 2),
            texture_stage_state(stage, 5, 1),
            texture_stage_state(stage, 14, glow.uv_set),
        ]);
        commands.extend(netimmerse_sampler_states(stage, glow)?);
    }

    if let Some(alpha_cutoff) = material.alpha_cutoff {
        commands.extend([
            effect_state(4, 1),
            effect_state(10, (alpha_cutoff * 255.0).round() as u32),
            effect_state(11, d3d_compare_function(material.alpha_test_mode)?),
        ]);
    }
    if material.alpha_blend {
        commands.extend([
            render_state(27, 1),
            render_state(19, d3d_blend_function(material.source_blend_mode)?),
            render_state(20, d3d_blend_function(material.destination_blend_mode)?),
        ]);
    }
    commands.extend([
        render_state(7, u32::from(material.depth_test)),
        render_state(14, u32::from(material.depth_write)),
        render_state(
            23,
            if material.depth_test {
                d3d_compare_function(material.depth_test_mode)?
            } else {
                8
            },
        ),
        render_state(22, d3d_cull_mode(material.cull_mode)?),
    ]);

    Ok(LoweredNetImmerseFixedFunctionMaterial {
        evaluated_effect: EvaluatedD3d9Effect {
            parameter_descriptions: Box::default(),
            evaluated_techniques: Box::new([EvaluatedD3d9EffectTechnique {
                technique_name: "NetImmerseFixedFunction".to_owned(),
                is_valid: true,
                quality_annotation: None,
                evaluated_passes: Box::new([EvaluatedD3d9EffectPass {
                    pass_name: "NetImmerseFixedFunction".to_owned(),
                    evaluated_commands: commands.into_boxed_slice(),
                }]),
            }]),
        },
        texture_asset_paths,
    })
}

fn material_state(index: u32, values: [f32; 4]) -> EvaluatedD3d9EffectCommand {
    EvaluatedD3d9EffectCommand::D3d9MaterialState {
        material_state: index,
        material_values: values,
    }
}

fn effect_state(index: u32, value: u32) -> EvaluatedD3d9EffectCommand {
    EvaluatedD3d9EffectCommand::UnmappedD3d9EffectState {
        state_type: index,
        state_index: 0,
        state_value: value,
    }
}

fn render_state(index: u32, value: u32) -> EvaluatedD3d9EffectCommand {
    EvaluatedD3d9EffectCommand::D3d9RenderState {
        render_state: index,
        state_value: value,
    }
}

fn texture_binding(stage: u32, parameter: String) -> EvaluatedD3d9EffectCommand {
    EvaluatedD3d9EffectCommand::D3d9TextureBinding {
        texture_stage: stage,
        parameter_name: Some(parameter),
    }
}

fn texture_stage_state(texture_index: u32, state: u32, value: u32) -> EvaluatedD3d9EffectCommand {
    EvaluatedD3d9EffectCommand::D3d9TextureStageState {
        texture_stage: texture_index,
        texture_stage_state: state,
        state_value: value,
    }
}

fn sampler_state(texture_index: u32, state: u32, value: u32) -> EvaluatedD3d9EffectCommand {
    EvaluatedD3d9EffectCommand::D3d9SamplerState {
        sampler_index: texture_index,
        sampler_state: state,
        state_value: value,
    }
}

fn netimmerse_sampler_states(
    stage: u32,
    texture: &super::model::NativeTextureSource,
) -> io::Result<[EvaluatedD3d9EffectCommand; 5]> {
    let (address_u, address_v) = match texture.clamp_mode {
        0 => (3, 3),
        1 => (3, 1),
        2 => (1, 3),
        3 => (1, 1),
        mode => return Err(invalid_native_material_state("texture clamp mode", mode)),
    };
    let (enlargement_filter, reduction_filter, mip_sampling) = match texture.filter_mode {
        0 => (1, 1, 0),
        1 => (2, 2, 0),
        2 => (2, 2, 2),
        3 => (1, 1, 1),
        4 => (1, 1, 2),
        5 => (2, 2, 1),
        6 => (3, 3, 2),
        mode => return Err(invalid_native_material_state("texture filter mode", mode)),
    };
    Ok([
        sampler_state(stage, 1, address_u),
        sampler_state(stage, 2, address_v),
        sampler_state(stage, 5, enlargement_filter),
        sampler_state(stage, 6, reduction_filter),
        sampler_state(stage, 7, mip_sampling),
    ])
}

fn d3d_blend_function(netimmerse_mode: u32) -> io::Result<u32> {
    [2, 1, 3, 4, 9, 10, 5, 6, 7, 8, 11]
        .get(netimmerse_mode as usize)
        .copied()
        .ok_or_else(|| invalid_native_material_state("alpha blend mode", netimmerse_mode))
}

fn d3d_compare_function(netimmerse_mode: u32) -> io::Result<u32> {
    [8, 2, 3, 4, 5, 6, 7, 1]
        .get(netimmerse_mode as usize)
        .copied()
        .ok_or_else(|| invalid_native_material_state("alpha test mode", netimmerse_mode))
}

fn d3d_cull_mode(netimmerse_mode: u32) -> io::Result<u32> {
    // Gamebryo's DX9 renderer maps CCW_OR_BOTH, CCW, CW, and BOTH to
    // D3DCULL_CW, D3DCULL_CW, D3DCULL_CCW, and D3DCULL_NONE.
    [2, 2, 3, 1]
        .get(netimmerse_mode as usize)
        .copied()
        .ok_or_else(|| invalid_native_material_state("cull mode", netimmerse_mode))
}

fn invalid_native_material_state(kind: &str, value: u32) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported NetImmerse {kind} {value}"),
    )
}
