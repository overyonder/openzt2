//! Blue Fang DAT and supporting source-document loading into one terrain asset.

use std::{collections::HashMap, io};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    gltf::{GltfLoader, GltfLoaderSettings},
    image::ImageLoaderSettings,
    prelude::*,
};

use crate::assets::{
    model::bevy_gltf_loader_configuration::create_bevy_gltf_loader_for_lowered_source_assets,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    },
    terrain::source::{
        blue_fang_terrain_glb_and_domain_lowering::lower_blue_fang_terrain_to_domain_grid_and_glb,
        blue_fang_terrain_presentation_source_lowering::{
            find_authored_terrain_water_material_source_document_paths,
            find_authored_terrain_waterfall_particle_source_document_path,
            lower_authored_terrain_presentation_and_biomes_and_waterfall,
            read_authored_terrain_base_height_metres,
        },
        blue_fang_terrain_source_parsing::parse_blue_fang_terrain_source,
    },
};

use super::terrain_asset_types_and_borrowing_queries::TerrainAsset;

#[derive(TypePath)]
pub(super) struct TerrainSourceAssetLoader {
    gltf_loader: GltfLoader,
    asset_archives: crate::asset_source::AssetArchives,
}

impl FromWorld for TerrainSourceAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
            gltf_loader: create_bevy_gltf_loader_for_lowered_source_assets(world),
        }
    }
}

impl AssetLoader for TerrainSourceAssetLoader {
    type Asset = TerrainAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .asset_archives
            .measure_scene_loading_asset_translation("terrain");
        let mut terrain_source_bytes = Vec::new();
        reader.read_to_end(&mut terrain_source_bytes).await?;
        let terrain_source =
            parse_blue_fang_terrain_source(&terrain_source_bytes).map_err(io::Error::other)?;
        let mut source_documents = Vec::<OrderedSourceDocument>::new();
        let supporting_source_paths = std::iter::once("world/terrain.xml".to_owned())
            .chain(
                terrain_source
                    .biome_names()
                    .iter()
                    .filter(|biome_name| !biome_name.eq_ignore_ascii_case("gridland"))
                    .map(|biome_name| format!("biomes/{biome_name}.xml")),
            )
            .chain(std::iter::once("world/water/waterfall.xml".to_owned()));
        for supporting_source_path in supporting_source_paths {
            let supporting_source_bytes = load_context
                .read_asset_bytes(supporting_source_path.clone())
                .await
                .map_err(|error| io::Error::other(format!("{supporting_source_path}: {error}")))?;
            source_documents.push(
                parse_blue_fang_source_document(
                    AssetPath::new(&supporting_source_path),
                    &supporting_source_bytes,
                )
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
        for water_material_source_path in
            find_authored_terrain_water_material_source_document_paths(
                &source_documents,
                terrain_source.biome_names(),
            )?
        {
            let water_material_source_bytes = load_context
                .read_asset_bytes(water_material_source_path.clone())
                .await
                .map_err(|error| {
                    io::Error::other(format!("{water_material_source_path}: {error}"))
                })?;
            source_documents.push(
                parse_blue_fang_source_document(
                    AssetPath::new(&water_material_source_path),
                    &water_material_source_bytes,
                )
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
        if let Some(waterfall_particle_source_path) =
            find_authored_terrain_waterfall_particle_source_document_path(&source_documents)?
        {
            let waterfall_particle_source_bytes = load_context
                .read_asset_bytes(waterfall_particle_source_path.clone())
                .await
                .map_err(|error| {
                    io::Error::other(format!("{waterfall_particle_source_path}: {error}"))
                })?;
            source_documents.push(
                parse_blue_fang_source_document(
                    AssetPath::new(&waterfall_particle_source_path),
                    &waterfall_particle_source_bytes,
                )
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
        let (terrain_presentation, terrain_biomes, terrain_waterfall) =
            lower_authored_terrain_presentation_and_biomes_and_waterfall(
                &source_documents,
                terrain_source.biome_names(),
            )?;
        let lowered_terrain = lower_blue_fang_terrain_to_domain_grid_and_glb(
            terrain_source,
            read_authored_terrain_base_height_metres(&source_documents)?,
            terrain_presentation,
            terrain_biomes,
            terrain_waterfall,
        )
        .map_err(io::Error::other)?;
        let canonical_terrain_grid = lowered_terrain.grid;
        let authored_water_region_membership =
            TerrainAsset::index_authored_water_region_membership(&canonical_terrain_grid)?;
        let lowered_terrain_model = GltfLoader::load_gltf(
            &self.gltf_loader,
            &lowered_terrain.glb,
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
        let terrain_texture_source_paths = canonical_terrain_grid.biomes.iter().flat_map(|biome| {
            std::iter::once(&biome.ground_texture)
                .chain(std::iter::once(&biome.cover_texture))
                .chain(std::iter::once(&biome.mix_mask))
                .chain(biome.cliff_textures.iter())
                .chain(biome.ground_brushes.iter())
                .chain(biome.cover_brushes.iter())
                .chain(biome.ground_no_blend_brushes.iter())
                .chain(biome.cover_no_blend_brushes.iter())
                .chain(biome.water_presentation.iter().flat_map(|water| {
                    std::iter::once(&water.gloss_map).chain(
                        water
                            .texture_animations
                            .iter()
                            .map(|animation| &animation.texture),
                    )
                }))
        });
        let waterfall_texture_source_paths = canonical_terrain_grid
            .waterfall
            .iter()
            .flat_map(|waterfall| [&waterfall.decal_mask, &waterfall.decal_detail]);
        let texture_images_by_normalized_source_path = terrain_texture_source_paths
            .chain(std::iter::once(
                &canonical_terrain_grid.presentation.detail_texture,
            ))
            .chain(waterfall_texture_source_paths)
            .map(|source_path| {
                self.asset_archives
                    .resolve_ui_reference(load_context.path().path(), source_path)
                    .map(|resolved_source_path| {
                        (
                            source_path.to_ascii_lowercase(),
                            load_context
                                .load_builder()
                                .with_settings(|settings: &mut ImageLoaderSettings| {
                                    settings.is_srgb = false;
                                })
                                .load(crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(
                                    &resolved_source_path.to_string_lossy(),
                                )),
                        )
                    })
                    .ok_or_else(|| missing_terrain_reference("texture", source_path))
            })
            .collect::<io::Result<HashMap<_, _>>>()?;
        let waterfall_scene_prefab = canonical_terrain_grid
            .waterfall
            .as_ref()
            .map(|waterfall| load_context.load(waterfall.particle_scene.clone()));
        let waterfall_audio = canonical_terrain_grid
            .waterfall
            .as_ref()
            .and_then(|waterfall| waterfall.sound.as_ref())
            .map(|source_path| {
                self.asset_archives
                    .resolve_audio_reference(load_context.path().path(), source_path)
                    .map(|resolved_source_path| load_context.load(resolved_source_path))
                    .ok_or_else(|| missing_terrain_reference("waterfall sound", source_path))
            })
            .transpose()?;
        Ok(TerrainAsset {
            canonical_terrain_grid,
            authored_water_region_membership,
            lowered_terrain_model,
            terrain_effect_material,
            water_bump_combination_effect_material,
            water_surface_effect_material,
            waterfall_decal_effect_material,
            texture_images_by_normalized_source_path,
            waterfall_scene_prefab,
            waterfall_audio,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["dat"]
    }
}

fn missing_terrain_reference(reference_kind: &str, source_path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("terrain references missing {reference_kind} {source_path}"),
    )
}
