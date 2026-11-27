//! Blue Fang DAT and supporting source-document loading into one terrain asset.

use std::{collections::BTreeSet, io, path::Path};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use crate::{
    asset_source::{generated_asset_publication::GeneratedAssetPublication, AssetArchives},
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    },
    terrain::source::{
        blue_fang_terrain_glb_and_domain_lowering::lower_blue_fang_terrain_to_canonical_grid,
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

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TerrainSourceAssetSettings {
    pub tank_water_source_paths: Vec<std::path::PathBuf>,
}

#[derive(TypePath)]
pub(super) struct TerrainSourceAssetLoader {
    asset_archives: crate::asset_source::AssetArchives,
}

impl FromWorld for TerrainSourceAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
        }
    }
}

impl AssetLoader for TerrainSourceAssetLoader {
    type Asset = TerrainAsset;
    type Settings = TerrainSourceAssetSettings;
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_terrain", skip_all)
    )]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
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
        load_water_configuration_documents(
            &self.asset_archives,
            &mut GeneratedAssetPublication::Labeled(load_context),
            &mut source_documents,
        )
        .await?;
        let (terrain_presentation, terrain_biomes, terrain_waterfall) =
            lower_authored_terrain_presentation_and_biomes_and_waterfall(
                &source_documents,
                terrain_source.biome_names(),
            )?;
        let canonical_terrain_grid = lower_blue_fang_terrain_to_canonical_grid(
            terrain_source,
            read_authored_terrain_heights(&source_documents)?,
            terrain_presentation,
            terrain_biomes,
            terrain_waterfall,
        )
        .map_err(io::Error::other)?;
        super::terrain_asset_dependency_construction::build_terrain_asset_dependencies(
            canonical_terrain_grid,
            &self.asset_archives,
            &mut GeneratedAssetPublication::Labeled(load_context),
            &settings.tank_water_source_paths,
        )
        .await
    }
}

pub(super) async fn load_water_configuration_documents(
    archives: &AssetArchives,
    context: &mut GeneratedAssetPublication<'_, '_>,
    documents: &mut Vec<OrderedSourceDocument>,
) -> io::Result<()> {
    let mut seen: BTreeSet<_> = documents
        .iter()
        .map(|document| document.path.key())
        .collect();
    loop {
        let paths = referenced_configuration_paths(documents)
            .into_iter()
            .filter(|path| seen.insert(AssetPath::new(path).key()))
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Ok(());
        }
        for path in paths {
            let bytes = context.read_source(archives, Path::new(&path)).await?;
            documents.push(
                parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
    }
}
