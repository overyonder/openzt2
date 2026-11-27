//! Shared dependency construction for canonical terrain assets.
use super::terrain_asset_types_and_borrowing_queries::{
    ShorelineRippleCondition, ShorelineRippleScene, TankWaterMaterials, TerrainAsset,
};
use crate::graphics_settings_types::ThreeLevelGraphicsDetail;
use crate::{
    asset_source::{generated_asset_publication::GeneratedAssetPublication, AssetArchives},
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_data::{
    terrain::{TerrainBiomeWaterPresentation, TerrainGrid},
    AssetId,
};
use std::{collections::BTreeSet, io, path::Path, time::Duration};

pub(super) async fn build_terrain_asset_dependencies(
    canonical_terrain_grid: TerrainGrid,
    asset_archives: &AssetArchives,
    load_context: &mut GeneratedAssetPublication<'_, '_>,
    configured_tank_water_paths: &[std::path::PathBuf],
) -> io::Result<TerrainAsset> {
    let authored_water_region_membership =
        TerrainAsset::index_authored_water_region_membership(&canonical_terrain_grid)?;
    let chunk_meshes = super::source::canonical_terrain_grid_mesh_construction::construct_canonical_terrain_chunk_meshes(&canonical_terrain_grid)
        .map_err(io::Error::other)?
        .into_iter()
        .enumerate()
        .map(|(index, mesh)| load_context.publish(format!("terrain-chunk-{index}"), mesh))
        .collect();
    let terrain_effect_material = load_context.load("Materials/terrain.bfmat");
    let water_bump_combination_effect_material = load_context.load("Materials/waterblit.bfmat");
    let water_surface_effect_material = load_context.load("Materials/waterflat.bfmat");
    let waterfall_decal_effect_material = load_context.load("Materials/terraindecal.bfmat");
    let water_low_detail_materials = super::terrain_water_low_detail_material_construction::create_terrain_water_low_detail_materials(
            &canonical_terrain_grid, asset_archives, load_context,
        )?;
    let water_medium_detail_materials = super::terrain_water_low_detail_material_construction::create_terrain_water_medium_detail_materials(
            &canonical_terrain_grid, asset_archives, load_context,
        )?;
    let tank_water_materials = load_tank_water_materials(
        &canonical_terrain_grid,
        asset_archives,
        load_context,
        &water_surface_effect_material,
        [&water_low_detail_materials, &water_medium_detail_materials],
        configured_tank_water_paths,
    )
    .await?;
    let water_materials_by_normalized_source_path: HashMap<_, _> = [
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
        .chain(
            tank_water_materials
                .values()
                .map(|materials| &materials.presentation),
        )
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
    let shoreline_ripples =
        load_shoreline_ripple_conditions(&canonical_terrain_grid, asset_archives, load_context)
            .await?;
    let terrain_water_texture_source_paths = canonical_terrain_grid
        .biomes
        .iter()
        .filter_map(|biome| biome.water_presentation.as_ref())
        .chain(
            tank_water_materials
                .values()
                .map(|materials| &materials.presentation),
        )
        .flat_map(water_texture_paths);
    let terrain_texture_source_paths = canonical_terrain_grid
        .biomes
        .iter()
        .flat_map(|biome| {
            std::iter::once(&biome.ground_texture)
                .chain(std::iter::once(&biome.cover_texture))
                .chain(std::iter::once(&biome.mix_mask))
                .chain(biome.cliff_textures.iter())
                .chain(biome.ground_brushes.iter())
                .chain(biome.cover_brushes.iter())
                .chain(biome.ground_no_blend_brushes.iter())
                .chain(biome.cover_no_blend_brushes.iter())
        })
        .chain(terrain_water_texture_source_paths);
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
                    .resolve_ui_reference(load_context.source_path(), source_path)
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
                            asset_archives.resolve_ui_reference(load_context.source_path(), fallback)
                        }))
                    .or_else(|| canonical_terrain_grid.biomes.iter()
                        .filter_map(|biome| biome.water_presentation.as_ref())
                        .find(|water| {
                            let caustics = &water.surface_material.configuration.caustics;
                            caustics.bottom_map == source_path || caustics.top_map == source_path
                        }).and_then(|_| asset_archives.resolve_ui_reference(load_context.source_path(), "Materials/CausticTest")))
                    .map(|resolved_source_path| {
                        (
                            source_path.to_ascii_lowercase(),
                            load_context.load_image(
                                crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(&resolved_source_path.to_string_lossy()),
                                false,
                            ),
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
            asset_archives.resolve_ui_reference(load_context.source_path(), source_path)
        {
            let image = load_context.load_image(
                crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(&resolved.to_string_lossy()), false,
            );
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
                .resolve_audio_reference(load_context.source_path(), source_path)
                .map(|resolved_source_path| load_context.load(resolved_source_path))
                .ok_or_else(|| missing_terrain_reference("waterfall sound", source_path))
        })
        .transpose()?;
    Ok(TerrainAsset {
        named_water_particle_sources: super::source::blue_fang_terrain_presentation_source_lowering::index_named_water_particle_sources(asset_archives.source_paths_under(std::path::Path::new("particle"))),
        canonical_terrain_grid,
        authored_water_region_membership,
        tank_water_materials,
        chunk_meshes,
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
        shoreline_ripples,
    })
}
fn water_texture_paths(water: &TerrainBiomeWaterPresentation) -> impl Iterator<Item = &String> {
    let configuration = &water.surface_material.configuration;
    std::iter::once(&water.gloss_map)
        .chain(
            water
                .texture_animations
                .iter()
                .map(|animation| &animation.texture),
        )
        .chain(water.shoreline.submerged_texture.iter())
        .chain(water.shoreline.shore_texture.iter())
        .chain(configuration.scum.iter().flat_map(|scum| &scum.image))
        .chain(configuration.shore.iter().flat_map(|shore| {
            shore
                .ripple_top
                .iter()
                .chain(&shore.ripple_bottom)
                .chain(&shore.ripple_mask)
        }))
        .chain([
            &configuration.caustics.bottom_map,
            &configuration.caustics.top_map,
            &configuration.caustics.height_lookup,
        ])
}

async fn load_tank_water_materials(
    grid: &TerrainGrid,
    archives: &AssetArchives,
    context: &mut GeneratedAssetPublication<'_, '_>,
    default_high: &Handle<crate::material::material_asset_types::MaterialAsset>,
    fixed_materials: [&[Option<Handle<crate::material::material_asset_types::MaterialAsset>>]; 2],
    configured_paths: &[std::path::PathBuf],
) -> io::Result<HashMap<AssetId, TankWaterMaterials>> {
    use super::source::blue_fang_terrain_presentation_source_lowering::{
        lower_authored_water_material_document, lower_water_presentation,
    };
    use super::terrain_source_asset_loading::load_water_configuration_documents;
    use super::terrain_water_low_detail_material_construction::create_water_fixed_detail_material;

    let default_path = "materials/waterflat.xml";
    let mut biome_appearances = std::collections::BTreeMap::new();
    for (index, biome) in grid.biomes.iter().enumerate() {
        let Some(presentation) = &biome.water_presentation else {
            continue;
        };
        let path = format!("biomes/{}.xml", biome.name.to_ascii_lowercase());
        if !archives.source_exists(Path::new(&path)) {
            continue;
        }
        let bytes = context.read_source(archives, Path::new(&path)).await?;
        let document = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
            .map_err(io::Error::other)?;
        if let Some(water) = document
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("water"))
        {
            let key = water
                .attribute("waterXML")
                .filter(|path| !path.is_empty())
                .map(|path| AssetPath::new(path).key())
                .unwrap_or_else(|| default_path.to_owned());
            biome_appearances
                .entry(key)
                .or_insert((index, presentation));
        }
    }
    let paths: BTreeSet<_> =
        archives
            .source_paths_under(Path::new("materials"))
            .into_iter()
            .chain(archives.source_paths_under(Path::new("world/water")))
            .chain(biome_appearances.keys().map(std::path::PathBuf::from))
            .chain(configured_paths.iter().map(|path| {
                std::path::PathBuf::from(AssetPath::new(&path.to_string_lossy()).key())
            }))
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            })
            .collect();
    let mut result = HashMap::new();
    for path in paths {
        let key = AssetPath::new(&path.to_string_lossy()).key();
        let bytes = context.read_source(archives, &path).await?;
        let required = configured_paths
            .iter()
            .any(|path| AssetPath::new(&path.to_string_lossy()).key() == key);
        let root_document = match parse_blue_fang_source_document(AssetPath::new(&key), &bytes) {
            Ok(document) if document.root.name.eq_ignore_ascii_case("water") => document,
            Ok(_) if required => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{key} is not a water material document"),
                ));
            }
            Err(error) if required => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, error));
            }
            _ => continue,
        };
        let mut source_documents = vec![root_document];
        load_water_configuration_documents(archives, context, &mut source_documents).await?;
        let root = &source_documents[0].root;
        let has_texture_animations = root
            .element_children()
            .any(|child| child.name.eq_ignore_ascii_case("texture"));
        let surface =
            lower_authored_water_material_document(&source_documents[0], &source_documents)?;
        let presentation = if has_texture_animations {
            lower_water_presentation(root, surface)?
        } else if let Some(presentation) = biome_appearances
            .get(&key)
            .or_else(|| biome_appearances.get(default_path))
            .map(|(_, presentation)| *presentation)
            .or_else(|| {
                grid.biomes
                    .iter()
                    .find_map(|biome| biome.water_presentation.as_ref())
            })
        {
            let mut presentation = presentation.clone();
            presentation.surface_material = surface;
            presentation
        } else {
            continue;
        };
        let id = AssetId::from_virtual_path(&key);
        let label = format!("tank-water-{}", id.to_lowercase_hexadecimal_string());
        let cached_index = biome_appearances
            .get(&key)
            .filter(|(_, biome)| **biome == presentation)
            .map(|&(index, _)| index);
        let mut fixed = |quality: usize| {
            if let Some(handle) =
                cached_index.and_then(|index| fixed_materials[quality][index].clone())
            {
                Ok(handle)
            } else {
                create_water_fixed_detail_material(
                    &presentation,
                    archives,
                    context,
                    &label,
                    quality == 1,
                )
            }
        };
        let low = fixed(0)?;
        let medium = fixed(1)?;
        let high_path = path.with_extension("bfmat");
        let high = if archives.source_exists(&high_path) {
            context.load(high_path)
        } else {
            default_high.clone()
        };
        result.insert(
            id,
            TankWaterMaterials {
                presentation,
                low,
                medium,
                high,
            },
        );
    }
    Ok(result)
}

fn missing_terrain_reference(reference_kind: &str, source_path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("terrain references missing {reference_kind} {source_path}"),
    )
}

async fn load_shoreline_ripple_conditions(
    grid: &TerrainGrid,
    archives: &AssetArchives,
    context: &mut GeneratedAssetPublication<'_, '_>,
) -> io::Result<HashMap<String, Vec<ShorelineRippleCondition>>> {
    let systems: BTreeSet<_> = grid
        .biomes
        .iter()
        .filter_map(|biome| biome.water_presentation.as_ref())
        .filter_map(|water| water.shoreline.ripple_system.as_ref())
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let mut ripples = HashMap::new();
    let dictionary_path = Path::new("world/particlesystems/maindict.xml");
    if systems.is_empty() || !archives.source_exists(dictionary_path) {
        return Ok(ripples);
    }
    let bytes = context.read_source(archives, dictionary_path).await?;
    let dictionary =
        parse_blue_fang_source_document(AssetPath::new(dictionary_path.to_string_lossy()), &bytes)
            .map_err(io::Error::other)?;
    for entry in dictionary.root.element_children() {
        let system = entry.name.to_ascii_lowercase();
        if !systems.contains(&system) {
            continue;
        }
        let mut conditions = Vec::new();
        for condition in entry
            .element_children()
            .filter(|node| node.name.eq_ignore_ascii_case("Conditions"))
        {
            if condition
                .attribute_named_any(&["ground"])
                .is_some_and(|ground| !ground.eq_ignore_ascii_case("water"))
            {
                continue;
            }
            let graphics = match condition
                .attribute_named_any(&["graphics"])
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                None => None,
                Some("low" | "lowest") => Some(ThreeLevelGraphicsDetail::Low),
                Some("med" | "medium") => Some(ThreeLevelGraphicsDetail::Medium),
                Some("high") => Some(ThreeLevelGraphicsDetail::High),
                Some(_) => continue,
            };
            let biome = condition
                .attribute_named_any(&["biome"])
                .map(str::to_ascii_lowercase);
            let mut instance = None;
            if let Some(particle) = condition
                .element_children()
                .find(|node| node.name.eq_ignore_ascii_case("BFParticleDefinition"))
            {
                let reference = particle
                    .attribute_named_any(&["xmlFile"])
                    .ok_or_else(|| missing_terrain_reference("shore ripple scene", &system))?;
                let source = archives
                    .resolve_source_reference(dictionary_path, reference)
                    .ok_or_else(|| missing_terrain_reference("shore ripple scene", reference))?;
                let bytes = context.read_source(archives, &source).await?;
                let scene = parse_blue_fang_source_document(
                    AssetPath::new(source.to_string_lossy()),
                    &bytes,
                )
                .map_err(io::Error::other)?;
                let model = scene
                    .root
                    .element_children()
                    .find(|node| node.name.eq_ignore_ascii_case("BFSceneGraphComponent"))
                    .and_then(|node| node.attribute_named_any(&["modelfile"]))
                    .ok_or_else(|| {
                        missing_terrain_reference("shore ripple model declaration", reference)
                    })?;
                let model = archives
                    .resolve_model_reference(&source, model)
                    .ok_or_else(|| missing_terrain_reference("shore ripple model", model))?;
                let lifetime = particle.attribute_named_any(&["removeTime"])
                    .and_then(crate::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme::<f64>)
                    .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
                    .filter(|duration| !duration.is_zero())
                    .ok_or_else(|| missing_terrain_reference("shore ripple removal time", reference))?;
                let scene = crate::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path(&model.to_string_lossy());
                instance = Some(ShorelineRippleScene {
                    scene: context.load(scene),
                    lifetime,
                });
            }
            conditions.push(ShorelineRippleCondition {
                graphics,
                biome,
                instance,
            });
        }
        ripples.insert(system, conditions);
    }
    Ok(ripples)
}
