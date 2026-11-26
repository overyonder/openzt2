//! Native NIF/BFB dispatch into renderer-neutral model, scene, and particle products.

use std::path::Path;

use anyhow::Result;
use openzt2_game_data::scene_prefab::ScenePrefabDocument;

use super::{
    blue_fang_bfb_source::document::BlueFangBfbDocument,
    conversion_error::ConversionError,
    native_geometry::{
        bfb_geometry_and_skin_lowering::lower_blue_fang_render_parts,
        native_geometry_identity::netimmerse_geometry_skin_variant_path,
        nif_geometry_lowering::lower_netimmerse_geometry_block,
        nif_material_lowering::lower_netimmerse_material_animations,
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
    netimmerse_bone_level_of_detail_selection::NetImmerseBoneLod,
    netimmerse_fixed_function_material_effect_lowering::lower_netimmerse_material_into_evaluated_d3d9_effect,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
    nif_embedded_texture_lowering::{
        embedded_nif_texture_label, lower_embedded_nif_texture, referenced_embedded_nif_texture,
    },
    particles,
};

#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
pub fn lower_native_model_source(
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
            let bone_lod = NetImmerseBoneLod::from_document(&netimmerse_document);
            let mut renderer_neutral_models = Vec::new();
            let mut alternate_models = Vec::new();
            for block in netimmerse_document.blocks() {
                if !matches!(
                    &block.payload,
                    NetImmerseNifBlockPayload::NiTriShape(_)
                        | NetImmerseNifBlockPayload::NiTriStrips(_)
                ) {
                    continue;
                }
                let block_ref = i32::try_from(block.index)?;
                let initial_node = bone_lod
                    .nodes
                    .get(&block_ref)
                    .is_none_or(|node| node.initial == node.ordinal);
                let mut lower_geometry = |skin| {
                    lower_netimmerse_geometry_block(
                        &netimmerse_document,
                        block.index,
                        skin,
                        |material_reference| {
                            Some(openzt2_game_data::AssetId::from_key(
                                &material_reference.asset_key(),
                            ))
                        },
                        &mut resolve_texture_asset_path,
                    )
                };
                if let Some(shape) = bone_lod.shapes.get(&block_ref) {
                    for (&ordinal, &skin) in &shape.skins {
                        let mut lowered = lower_geometry(Some(skin))?;
                        if ordinal != shape.initial {
                            lowered.virtual_path = netimmerse_geometry_skin_variant_path(
                                &netimmerse_document,
                                block.index,
                                ordinal,
                            );
                        }
                        if ordinal == shape.initial && initial_node {
                            renderer_neutral_models.push((block.index, lowered));
                        } else {
                            alternate_models.push((block.index, lowered));
                        }
                    }
                } else {
                    let lowered = (block.index, lower_geometry(None)?);
                    if initial_node {
                        renderer_neutral_models.push(lowered);
                    } else {
                        alternate_models.push(lowered);
                    }
                }
            }
            let has_default_scene = !renderer_neutral_models.is_empty();
            renderer_neutral_models.extend(alternate_models);
            let mut material_geometry = std::collections::BTreeSet::new();
            let materials = renderer_neutral_models
                .iter()
                .filter(|(geometry_block_index, _)| material_geometry.insert(*geometry_block_index))
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
                        texture_binding: crate::material::material_asset_types::NetImmerseMaterialTextureBinding {
                            name: material.name.clone(),
                            decal_stage: 1 + usize::from(material.detail_texture.is_some()) + usize::from(material.glow_texture.is_some()),
                            decal_uv_set: material.decal_0_texture.as_ref().map_or(0, |texture| texture.uv_set),
                        },
                        texture_coordinate_animations,
                        material_animations: lower_netimmerse_material_animations(&netimmerse_document, geometry_block_index)?,
                        labelled_asset_path: native_model_material_labelled_asset_path(
                            native_model_asset_path,
                            geometry_block_index,
                        ),
                        evaluated_d3d9_effect: lowered.evaluated_effect,
                        texture_asset_paths: lowered.texture_asset_paths,
                        reflection_projections: lowered.reflection_projections,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let embedded_textures = materials
                .iter()
                .flat_map(|material| &material.texture_asset_paths)
                .filter_map(|(_, path)| {
                    referenced_embedded_nif_texture(native_model_asset_path, path)
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(|reference| {
                    let image = lower_embedded_nif_texture(&netimmerse_document, reference)?;
                    Ok((embedded_nif_texture_label(reference), image))
                })
                .collect::<std::io::Result<Vec<_>>>()?;
            let renderer_neutral_models = renderer_neutral_models
                .into_iter()
                .map(|(_, model)| model)
                .collect();
            let mut scene_prefab_document = lower_netimmerse_scene_prefab(
                &netimmerse_document,
                &bone_lod,
                native_model_asset_path,
            )?;
            scene_prefab_document.automatic_placement_bounds_xz =
                lower_netimmerse_automatic_placement_bounds(&netimmerse_document)?;
            Ok(LoweredNativeModel {
                glb: encode_models_as_glb(renderer_neutral_models, has_default_scene)?,
                scene_prefab_document,
                particle_effects: particles::lower_all(
                    &netimmerse_document,
                    native_model_asset_path,
                )?,
                materials,
                embedded_textures,
            })
        }
        Some(extension) if extension.eq_ignore_ascii_case("bfb") => {
            let blue_fang_document = BlueFangBfbDocument::parse(
                native_model_asset_path.to_owned(),
                native_model_source_bytes,
            )?;
            let renderer_neutral_models =
                lower_blue_fang_render_parts(&blue_fang_document, |material_reference| {
                    Some(openzt2_game_data::AssetId::from_key(
                        &material_reference.asset_key(),
                    ))
                })?;
            let has_default_scene = renderer_neutral_models
                .first()
                .is_some_and(|(_, is_initial)| *is_initial);
            let renderer_neutral_models = renderer_neutral_models
                .into_iter()
                .map(|(model, _)| model)
                .collect();
            let mut scene_prefab_document =
                lower_blue_fang_scene_prefab(&blue_fang_document, native_model_asset_path)?;
            scene_prefab_document.automatic_placement_bounds_xz =
                lower_blue_fang_automatic_placement_bounds(&blue_fang_document)?;
            Ok(LoweredNativeModel {
                glb: encode_models_as_glb(renderer_neutral_models, has_default_scene)?,
                scene_prefab_document,
                particle_effects: Vec::new(),
                materials: Vec::new(),
                embedded_textures: Vec::new(),
            })
        }
        _ => Err(ConversionError::InvalidSource("unsupported native model source").into()),
    }
}

pub fn native_model_scene_labelled_asset_path(native_model_asset_path: &str) -> String {
    // Loader dispatch happens before the archive's case-insensitive lookup.
    let path = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(Path::new(
        native_model_asset_path,
    ));
    format!(
        "{}#Scene",
        super::gltf_model_source::gltf_model_asset_path_for_source_path(&path).display()
    )
}

pub(crate) fn lower_native_physical_scene_document(
    path: &str,
    bytes: &[u8],
) -> Result<ScenePrefabDocument> {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("nif") => {
            let document = NetImmerseNifDocument::parse(path.to_owned(), bytes)?;
            let bone_lod = NetImmerseBoneLod::from_document(&document);
            let mut prefab = lower_netimmerse_scene_prefab(&document, &bone_lod, path)?;
            prefab.automatic_placement_bounds_xz =
                lower_netimmerse_automatic_placement_bounds(&document)?;
            Ok(prefab)
        }
        Some(extension) if extension.eq_ignore_ascii_case("bfb") => {
            let document = BlueFangBfbDocument::parse(path.to_owned(), bytes)?;
            let mut prefab = lower_blue_fang_scene_prefab(&document, path)?;
            prefab.automatic_placement_bounds_xz =
                lower_blue_fang_automatic_placement_bounds(&document)?;
            Ok(prefab)
        }
        Some(extension)
            if extension.eq_ignore_ascii_case("glb") || extension.eq_ignore_ascii_case("gltf") =>
        {
            super::gltf_model_source::gltf_model_scene_prefab(path, bytes)
        }
        _ => Err(ConversionError::InvalidSource("unsupported native model source").into()),
    }
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
