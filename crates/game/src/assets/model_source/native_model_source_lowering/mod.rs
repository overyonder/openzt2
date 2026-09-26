//! Native NIF/BFB dispatch into renderer-neutral model, scene, and particle products.

use std::path::Path;

use anyhow::Result;

use super::{
    blue_fang_bfb_source::document::BlueFangBfbDocument,
    conversion_error::ConversionError,
    native_geometry::{
        bfb_geometry_and_skin_lowering::lower_blue_fang_render_parts_for_lod,
        nif_geometry_lowering::lower_netimmerse_geometry_block,
    },
    native_model_glb_encoding::encode_models_as_glb,
    native_model_source_lowering_types::{LoweredNativeModel, LoweredNativeModelMaterial},
    native_scene::{
        automatic_placement_bounds_lowering::{
            lower_blue_fang_automatic_placement_bounds, lower_netimmerse_automatic_placement_bounds,
        },
        bfb_scene_prefab_lowering::lower_blue_fang_scene_prefab,
        nif_scene_prefab_lowering::lower_netimmerse_scene_prefab,
    },
    netimmerse_bone_level_of_detail_selection::NetImmerseSelectedBoneLevelOfDetail,
    netimmerse_fixed_function_material_effect_lowering::lower_netimmerse_material_into_evaluated_d3d9_effect,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
    particles,
};

pub(in crate::assets) fn lower_native_model_source(
    native_model_asset_path: &str,
    native_model_source_bytes: &[u8],
    mut resolve_texture_asset_path: impl FnMut(&str) -> Option<String>,
) -> Result<LoweredNativeModel> {
    match Path::new(native_model_asset_path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("nif") => {
            let netimmerse_document = NetImmerseNifDocument::parse(
                native_model_asset_path.to_owned(),
                native_model_source_bytes,
            )?;
            let selected_bone_level_of_detail =
                NetImmerseSelectedBoneLevelOfDetail::from_document(&netimmerse_document);
            let renderer_neutral_models = netimmerse_document
                .blocks()
                .filter_map(|block| {
                    matches!(
                        &block.payload,
                        NetImmerseNifBlockPayload::NiTriShape(_)
                            | NetImmerseNifBlockPayload::NiTriStrips(_)
                    )
                    .then_some(block.index)
                    .filter(|index| {
                        i32::try_from(*index).is_ok_and(|block| {
                            selected_bone_level_of_detail.scene_block_is_visible(block)
                        })
                    })
                })
                .map(|geometry_block_index| {
                    lower_netimmerse_geometry_block(
                        &netimmerse_document,
                        &selected_bone_level_of_detail,
                        geometry_block_index,
                        |material_reference| {
                            Some(openzt2_game_data::AssetId::from_key(
                                &material_reference.asset_key(),
                            ))
                        },
                        &mut resolve_texture_asset_path,
                    )
                    .map(|model| (geometry_block_index, model))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let materials = renderer_neutral_models
                .iter()
                .filter_map(|(geometry_block_index, model)| {
                    model
                        .meshes
                        .iter()
                        .flat_map(|mesh| &mesh.submeshes)
                        .find_map(|submesh| submesh.native_material.as_ref())
                        .map(|material| (*geometry_block_index, material))
                })
                .map(|(geometry_block_index, material)| {
                    let texture_coordinate_animations = super::netimmerse_texture_coordinate_animation_lowering::lower_texture_coordinate_animations(
                        &netimmerse_document, geometry_block_index,
                    )?;
                    let mut lowered = lower_netimmerse_material_into_evaluated_d3d9_effect(material)?;
                    for technique in &mut lowered.evaluated_effect.evaluated_techniques {
                        for pass in &mut technique.evaluated_passes {
                            use d3d9_effects::effect_types::EvaluatedD3d9EffectCommand;
                            let animated_stages = pass.evaluated_commands.iter().filter_map(|command| match command {
                                EvaluatedD3d9EffectCommand::D3d9TextureStageState { texture_stage, texture_stage_state: 14, state_value }
                                    if texture_coordinate_animations.iter().any(|animation| u32::from(animation.uv_set) == *state_value) => Some(*texture_stage),
                                _ => None,
                            }).collect::<Vec<_>>();
                            let mut commands = std::mem::take(&mut pass.evaluated_commands).into_vec();
                            commands.extend(animated_stages.into_iter().map(|texture_stage| EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                                // Evaluated Effects state ordinal, not raw D3DTSS 24.
                                texture_stage, texture_stage_state: 17, state_value: 2,
                            }));
                            pass.evaluated_commands = commands.into_boxed_slice();
                        }
                    }
                    Ok(LoweredNativeModelMaterial {
                        texture_coordinate_animations,
                        labelled_asset_path: native_model_material_labelled_asset_path(
                            native_model_asset_path,
                            geometry_block_index,
                        ),
                        evaluated_d3d9_effect: lowered.evaluated_effect,
                        texture_asset_paths: lowered.texture_asset_paths,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let renderer_neutral_models = renderer_neutral_models
                .into_iter()
                .map(|(_, model)| model)
                .collect();
            let mut scene_prefab_document = lower_netimmerse_scene_prefab(
                &netimmerse_document,
                &selected_bone_level_of_detail,
                native_model_asset_path,
                None,
            )?;
            scene_prefab_document.automatic_placement_bounds_xz =
                lower_netimmerse_automatic_placement_bounds(&netimmerse_document)?;
            Ok(LoweredNativeModel {
                glb: encode_models_as_glb(renderer_neutral_models)?,
                scene_prefab_document,
                particle_effects: particles::lower_all(
                    &netimmerse_document,
                    native_model_asset_path,
                )?,
                materials,
            })
        }
        Some(extension) if extension.eq_ignore_ascii_case("bfb") => {
            let blue_fang_document = BlueFangBfbDocument::parse(
                native_model_asset_path.to_owned(),
                native_model_source_bytes,
            )?;
            let renderer_neutral_models = lower_blue_fang_render_parts_for_lod(
                &blue_fang_document,
                0,
                |material_reference| {
                    Some(openzt2_game_data::AssetId::from_key(
                        &material_reference.asset_key(),
                    ))
                },
            )?;
            let mut scene_prefab_document =
                lower_blue_fang_scene_prefab(&blue_fang_document, native_model_asset_path, None)?;
            scene_prefab_document.automatic_placement_bounds_xz =
                lower_blue_fang_automatic_placement_bounds(&blue_fang_document)?;
            Ok(LoweredNativeModel {
                glb: encode_models_as_glb(renderer_neutral_models)?,
                scene_prefab_document,
                particle_effects: Vec::new(),
                materials: Vec::new(),
            })
        }
        _ => Err(ConversionError::InvalidSource("unsupported native model source").into()),
    }
}

pub(in crate::assets) fn native_model_scene_labelled_asset_path(
    native_model_asset_path: &str,
) -> String {
    format!("{native_model_asset_path}#Scene")
}

pub(super) fn native_model_particle_effect_labelled_asset_path(
    native_model_asset_path: &str,
    netimmerse_block_index: u32,
) -> String {
    format!("{native_model_asset_path}#Effect/{netimmerse_block_index}")
}

pub(super) fn native_model_material_labelled_asset_path(
    native_model_asset_path: &str,
    netimmerse_block_index: u32,
) -> String {
    format!("{native_model_asset_path}#Material/nif_{netimmerse_block_index:08x}")
}

#[cfg(test)]
mod globe_material_regression_tests {
    use std::path::Path;

    use super::lower_native_model_source;
    use d3d9_effects::effect_types::EvaluatedD3d9EffectCommand;

    #[test]
    #[ignore = "requires the locally installed original Z2F"]
    fn native_globe_glow_texture_survives_lowering() -> anyhow::Result<()> {
        let directory = std::path::PathBuf::from(std::env::var("OPENZT2_Z2F_PATH")?);
        let archives = z2f::ArchiveSet::open([directory.join("x300_000.z2f")])?;
        let path = "ui/globe/mapdot_selected.nif";
        let lowered =
            lower_native_model_source(path, &archives.read(Path::new(path))?, |reference| {
                archives
                    .resolve_model_texture_reference(Path::new(path), reference)
                    .map(|resolved| resolved.to_string_lossy().into_owned())
            })?;
        assert!(lowered.materials.iter().any(|material| {
            material
                .texture_asset_paths
                .iter()
                .any(|(parameter, path)| {
                    parameter == "NetImmerseGlowTexture"
                        && path.eq_ignore_ascii_case("ui/globe/post.tga")
                })
                && material
                    .evaluated_d3d9_effect
                    .evaluated_techniques
                    .iter()
                    .flat_map(|technique| &technique.evaluated_passes)
                    .flat_map(|pass| &pass.evaluated_commands)
                    .any(|command| {
                        matches!(
                            command,
                            EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                                texture_stage: 1,
                                texture_stage_state: 1,
                                state_value: 7
                            }
                        )
                    })
        }));
        Ok(())
    }
}
