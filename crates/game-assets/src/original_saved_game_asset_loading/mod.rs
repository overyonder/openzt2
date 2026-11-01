//! Typed access to one original saved world and its own terrain dependency.

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_scenario::{
        saved_world_map_metadata::lower_saved_world_map_metadata,
        world_scenario_document_source::WorldScenarioDocumentSource,
        world_source_document_dependency_lowering::lower_saved_world_starting_document_with_scene_dependencies,
    },
};
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use std::io;

#[derive(Asset, TypePath, Debug)]
pub struct OriginalSavedGameAsset {
    pub document: OrderedSourceDocument,
    pub source: WorldScenarioDocumentSource,
    pub terrain: Handle<TerrainAsset>,
}

#[derive(TypePath)]
pub(crate) struct OriginalSavedGameAssetLoader {
    archives: AssetArchives,
}

impl FromWorld for OriginalSavedGameAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for OriginalSavedGameAssetLoader {
    type Asset = OriginalSavedGameAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document = parse_blue_fang_source_document(
            AssetPath::new(&context.path().path().to_string_lossy()),
            &bytes,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if document.root.name.as_str() != "saveRoot"
            || document
                .root
                .element_children()
                .filter(|node| node.name.as_str() == "BFContext")
                .count()
                != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "saved zoo must contain one BFContext",
            ));
        }
        let path = context
            .path()
            .clone()
            .resolve_embed_str("terrain.dat")
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let source_asset_id =
            openzt2_game_data::AssetId::from_virtual_path(&context.path().to_string());
        let mut source =
            lower_saved_world_map_metadata(&document, source_asset_id, &path.to_string(), None)?;
        let starting = lower_saved_world_starting_document_with_scene_dependencies(
            &document,
            &self.archives,
            context,
        )
        .await?;
        let mut starting_zoos = starting.document.starting_zoos;
        if starting_zoos.len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "saved world must contain one starting world",
            ));
        }
        let map = source.document.maps.first().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "saved world has no map metadata",
            )
        })?;
        starting_zoos[0].id = map.starting_zoo;
        starting_zoos[0].map = map.id;
        source.document.starting_zoos = starting_zoos;
        source.scenes = starting.scenes;
        let terrain = context.load(path);
        Ok(OriginalSavedGameAsset {
            document,
            source,
            terrain,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["zt2"]
    }
}

pub struct OriginalSavedGameAssetPlugin;

impl Plugin for OriginalSavedGameAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<OriginalSavedGameAsset>()
            .init_asset_loader::<OriginalSavedGameAssetLoader>();
    }
}
