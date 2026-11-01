//! Shared dependency construction for canonical terrain assets.
use super::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
    terrain::source::blue_fang_terrain_presentation_source_lowering::normalize_particle_source_document_name,
};
use bevy::{
    asset::LoadContext,
    gltf::{GltfLoader, GltfLoaderSettings},
    image::ImageLoaderSettings,
    prelude::*,
};
use openzt2_game_data::terrain::TerrainGrid;
use std::{collections::HashMap, io};

pub(super) async fn build_terrain_asset_dependencies(
    canonical_terrain_grid: TerrainGrid,
    glb: &[u8],
    gltf_loader: &GltfLoader,
    asset_archives: &AssetArchives,
    load_context: &mut LoadContext<'_>,
) -> io::Result<TerrainAsset> {
    let authored_water_region_membership =
        TerrainAsset::index_authored_water_region_membership(&canonical_terrain_grid)?;
    let lowered_terrain_model = GltfLoader::load_gltf(
        gltf_loader,
        glb,
        load_context,
        &GltfLoaderSettings::default(),
    )
    .await
    .map_err(io::Error::other)?;
    let lowered_terrain_model = load_context.add_labeled_asset("model", lowered_terrain_model);
    let terrain_effect_material = load_context.load("Materials/terrain.bfmat");
    let water_bump_combination_effect_material = load_context.load("Materials/waterblit.bfmat");
    let water_surface_effect_material = load_context.load("Materials/waterflat.bfmat");
    let waterfall_decal_effect_material = load_context.load("Materials/terraindecal.bfmat");
    let water_materials_by_normalized_source_path = [
        "Materials/waterflat.bfmat",
        "Materials/waterscum.bfmat",
        "Materials/watershore.bfmat",
        "Materials/caustic.bfmat",
        "Materials/causticterrain.bfmat",
        "Materials/causticdefault.bfmat",
        "Materials/WaterRay.bfmat",
        "Materials/WaterDisk.bfmat",
    ]
    .into_iter()
    .map(|path| (path.to_ascii_lowercase(), load_context.load(path)))
    .collect();
    let water_particle_effects_by_normalized_system = canonical_terrain_grid
        .biomes
        .iter()
        .filter_map(|biome| biome.water_presentation.as_ref())
        .flat_map(|water| {
            let configuration = &water.surface_material.configuration;
            configuration
                .spray
                .iter()
                .flat_map(|detail| detail.variants.iter().map(|variant| &variant.system))
                .chain(
                    configuration
                        .spray_fallback
                        .iter()
                        .map(|variant| &variant.system),
                )
                .chain(configuration.wake_systems.iter())
                .chain(configuration.high_wake_systems.iter())
                .chain(
                    configuration
                        .bubbles
                        .iter()
                        .filter_map(|detail| detail.system.as_ref()),
                )
                .chain(configuration.bubbles_fallback.system.iter())
                .chain(
                    configuration
                        .suspension
                        .iter()
                        .filter_map(|suspension| suspension.system.as_ref()),
                )
        })
        .filter(|system| !system.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|system| {
            let source_path = format!("particle/{system}.psys");
            (system.to_ascii_lowercase(), load_context.load(source_path))
        })
        .collect::<HashMap<_, _>>();
    let water_scene_source_paths =
        asset_archives.source_paths_under(std::path::Path::new("world/water"));
    let mut water_scene_prefabs_by_normalized_system = HashMap::new();
    for system in canonical_terrain_grid
        .biomes
        .iter()
        .filter_map(|biome| biome.water_presentation.as_ref())
        .filter_map(|water| water.shoreline.ripple_system.as_ref())
    {
        let key = normalize_particle_source_document_name(system);
        let source = water_scene_source_paths
            .iter()
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
                    && path
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| normalize_particle_source_document_name(name) == key)
            })
            .ok_or_else(|| missing_terrain_reference("shore ripple scene", system))?;
        let bytes = load_context
            .read_asset_bytes(source.clone())
            .await
            .map_err(|error| io::Error::other(format!("{}: {error}", source.display())))?;
        let declaration =
            parse_blue_fang_source_document(AssetPath::new(&source.to_string_lossy()), &bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let model = declaration
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("BFSceneGraphComponent"))
            .and_then(|node| node.attribute_named_any(&["modelfile"]))
            .ok_or_else(|| missing_terrain_reference("shore ripple model declaration", system))?;
        let model = asset_archives
            .resolve_ui_reference(source, model)
            .ok_or_else(|| missing_terrain_reference("shore ripple model", model))?;
        let scene = crate::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path(
                    &model.to_string_lossy().replace('\\', "/"),
                );
        water_scene_prefabs_by_normalized_system
            .insert(system.to_ascii_lowercase(), load_context.load(scene));
    }
    let terrain_texture_source_paths =
        canonical_terrain_grid.biomes.iter().flat_map(|biome| {
            std::iter::once(&biome.ground_texture)
                .chain(std::iter::once(&biome.cover_texture))
                .chain(std::iter::once(&biome.mix_mask))
                .chain(biome.cliff_textures.iter())
                .chain(biome.ground_brushes.iter())
                .chain(biome.cover_brushes.iter())
                .chain(biome.ground_no_blend_brushes.iter())
                .chain(biome.cover_no_blend_brushes.iter())
                .chain(biome.water_presentation.iter().flat_map(|water| {
                    std::iter::once(&water.gloss_map)
                        .chain(
                            water
                                .texture_animations
                                .iter()
                                .map(|animation| &animation.texture),
                        )
                        .chain(water.shoreline.submerged_texture.iter())
                        .chain(water.shoreline.shore_texture.iter())
                        .chain(water.surface_material.configuration.shore.iter().flat_map(
                            |shore| {
                                shore
                                    .ripple_top
                                    .iter()
                                    .chain(shore.ripple_bottom.iter())
                                    .chain(shore.ripple_mask.iter())
                            },
                        ))
                        .chain([
                            &water.surface_material.configuration.caustics.bottom_map,
                            &water.surface_material.configuration.caustics.top_map,
                            &water.surface_material.configuration.caustics.height_lookup,
                        ])
                }))
        });
    let waterfall_texture_source_paths = canonical_terrain_grid
        .waterfall
        .iter()
        .flat_map(|waterfall| [&waterfall.decal_mask, &waterfall.decal_detail]);
    let mut texture_images_by_normalized_source_path = terrain_texture_source_paths
            .chain(std::iter::once(
                &canonical_terrain_grid.presentation.detail_texture,
            ))
            .chain(waterfall_texture_source_paths)
            .map(String::as_str)
            .chain(["Materials/waterlayout", "Materials/watermask", "Materials/CausticTest"])
            .map(|source_path| {
                asset_archives
                    .resolve_ui_reference(load_context.path().path(), source_path)
                    .or_else(|| canonical_terrain_grid.biomes.iter()
                        .filter_map(|biome| biome.water_presentation.as_ref())
                        .filter_map(|water| water.surface_material.configuration.shore.as_ref())
                        .find_map(|shore| {
                            let fallback = if shore.ripple_mask.as_deref() == Some(source_path) {
                                "Materials/watermask"
                            } else if shore.ripple_top.as_deref() == Some(source_path)
                                || shore.ripple_bottom.as_deref() == Some(source_path) {
                                "Materials/waterlayout"
                            } else { return None; };
                            asset_archives.resolve_ui_reference(load_context.path().path(), fallback)
                        }))
                    .or_else(|| canonical_terrain_grid.biomes.iter()
                        .filter_map(|biome| biome.water_presentation.as_ref())
                        .find(|water| {
                            let caustics = &water.surface_material.configuration.caustics;
                            caustics.bottom_map == source_path || caustics.top_map == source_path
                        }).and_then(|_| asset_archives.resolve_ui_reference(load_context.path().path(), "Materials/CausticTest")))
                    .map(|resolved_source_path| {
                        (
                            source_path.to_ascii_lowercase(),
                            load_context
                                .load_builder()
                                .with_settings(|settings: &mut ImageLoaderSettings| {
                                    settings.is_srgb = false;
                                })
                                .load(crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(
                                    &resolved_source_path.to_string_lossy(),
                                )),
                        )
                    })
                    .map(Some)
                    .or_else(|| {
                        // `Biomes/benthic/benthic_shore_256.DDS` is authored by
                        // the shipped benthic biome but not shipped; the shore
                        // keeps the ground texture instead.
                        let optional_shore = canonical_terrain_grid.biomes.iter().any(|biome| {
                            biome.water_presentation.as_ref().is_some_and(|water| {
                                water.shoreline.shore_texture.as_deref() == Some(source_path)
                            })
                        });
                        optional_shore.then(|| {
                            warn!(texture = source_path, "terrain shore texture is missing; using the ground texture");
                            None
                        })
                    })
                    .ok_or_else(|| missing_terrain_reference("texture", source_path))
            })
            .filter_map(|entry| entry.transpose())
            .collect::<io::Result<HashMap<_, _>>>()?;
    for source_path in canonical_terrain_grid
        .biomes
        .iter()
        .filter_map(|biome| biome.water_presentation.as_ref())
        .filter_map(|water| water.surface_material.configuration.scum.as_ref())
        .filter_map(|scum| scum.image.as_ref())
    {
        if let Some(resolved) =
            asset_archives.resolve_ui_reference(load_context.path().path(), source_path)
        {
            let image = load_context.load_builder().with_settings(|settings: &mut ImageLoaderSettings| {
                    settings.is_srgb = false;
                }).load(crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(&resolved.to_string_lossy()));
            texture_images_by_normalized_source_path
                .insert(source_path.to_ascii_lowercase(), image);
        }
    }
    let waterfall_scene_prefab = canonical_terrain_grid
        .waterfall
        .as_ref()
        .map(|waterfall| load_context.load(waterfall.particle_scene.clone()));
    let waterfall_audio = canonical_terrain_grid
        .waterfall
        .as_ref()
        .and_then(|waterfall| waterfall.sound.as_ref())
        .map(|source_path| {
            asset_archives
                .resolve_audio_reference(load_context.path().path(), source_path)
                .map(|resolved_source_path| load_context.load(resolved_source_path))
                .ok_or_else(|| missing_terrain_reference("waterfall sound", source_path))
        })
        .transpose()?;
    let water_low_detail_materials = super::terrain_water_low_detail_material_construction::create_terrain_water_low_detail_materials(
            &canonical_terrain_grid, asset_archives, load_context,
        )?;
    let water_medium_detail_materials = super::terrain_water_low_detail_material_construction::create_terrain_water_medium_detail_materials(
            &canonical_terrain_grid, asset_archives, load_context,
        )?;
    Ok(TerrainAsset {
        canonical_terrain_grid,
        authored_water_region_membership,
        lowered_terrain_model,
        terrain_effect_material,
        water_bump_combination_effect_material,
        water_surface_effect_material,
        water_low_detail_materials,
        water_medium_detail_materials,
        waterfall_decal_effect_material,
        texture_images_by_normalized_source_path,
        waterfall_scene_prefab,
        waterfall_audio,
        water_materials_by_normalized_source_path,
        water_particle_effects_by_normalized_system,
        water_scene_prefabs_by_normalized_system,
    })
}
fn missing_terrain_reference(reference_kind: &str, source_path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("terrain references missing {reference_kind} {source_path}"),
    )
}
