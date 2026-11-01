//! Blue Fang DAT and supporting source-document loading into one terrain asset.

use std::{collections::HashSet, io};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    gltf::GltfLoader,
    prelude::*,
};

use crate::{
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
            read_authored_terrain_heights,
        },
        blue_fang_terrain_source_parsing::parse_blue_fang_terrain_source,
        water_configuration::referenced_configuration_paths,
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
        let mut loaded_configuration_paths = source_documents
            .iter()
            .map(|document| document.path.as_str().to_ascii_lowercase())
            .collect::<HashSet<_>>();
        loop {
            let paths = referenced_configuration_paths(&source_documents)
                .into_iter()
                .filter(|path| loaded_configuration_paths.insert(path.to_ascii_lowercase()))
                .collect::<Vec<_>>();
            if paths.is_empty() {
                break;
            }
            for path in paths {
                let bytes = load_context
                    .read_asset_bytes(path.clone())
                    .await
                    .map_err(|error| io::Error::other(format!("{path}: {error}")))?;
                source_documents.push(
                    parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
                );
            }
        }
        let (terrain_presentation, terrain_biomes, terrain_waterfall) =
            lower_authored_terrain_presentation_and_biomes_and_waterfall(
                &source_documents,
                terrain_source.biome_names(),
            )?;
        let lowered_terrain = lower_blue_fang_terrain_to_domain_grid_and_glb(
            terrain_source,
            read_authored_terrain_heights(&source_documents)?,
            terrain_presentation,
            terrain_biomes,
            terrain_waterfall,
        )
        .map_err(io::Error::other)?;
        let canonical_terrain_grid = lowered_terrain.grid;
        super::terrain_asset_dependency_construction::build_terrain_asset_dependencies(
            canonical_terrain_grid,
            &lowered_terrain.glb,
            &self.gltf_loader,
            &self.asset_archives,
            load_context,
        )
        .await
    }

    fn extensions(&self) -> &[&str] {
        &["dat"]
    }
}
