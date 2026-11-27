//! Typed access to one original saved world and its own terrain dependency.

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_scenario::{
        original_saved_world_configuration_loading::load_saved_world_configuration,
        saved_world_map_metadata::lower_saved_world_map_metadata,
        saved_world_scenario_asset_construction::build_restored_world_scenario_asset,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
        world_source_document_dependency_lowering::lower_saved_world_starting_document_with_scene_dependencies,
    },
};
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext, LoadedAsset},
    prelude::*,
};
use std::{collections::BTreeMap, io};

#[derive(Asset, TypePath, Debug)]
pub struct OriginalSavedGameAsset {
    pub document: OrderedSourceDocument,
    #[dependency]
    pub scenario: Handle<WorldScenarioDocumentAsset>,
    #[dependency]
    pub terrain: Handle<TerrainAsset>,
    #[dependency]
    pub economy: Handle<crate::world_definitions::world_definition_asset_set_refresh_and_indexing::saved_world_economy_asset::SavedWorldEconomyAsset>,
    pub unlimited_cash: bool,
    saved_entity_handles: BTreeMap<u32, u64>,
}

impl OriginalSavedGameAsset {
    pub fn persistent_id_for_saved_handle(&self, handle: u32) -> Option<u64> {
        self.saved_entity_handles.get(&handle).copied()
    }
}

#[derive(TypePath)]
pub(crate) struct OriginalSavedGameAssetLoader {
    archives: AssetArchives,
    server: AssetServer,
}

impl FromWorld for OriginalSavedGameAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
            server: world.resource::<AssetServer>().clone(),
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
        let manager = document
            .root
            .element_children()
            .find(|node| node.name == "BFContext")
            .and_then(|context| {
                context
                    .element_children()
                    .find(|node| node.name == "BFGManager")
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "saved world has no BFGManager")
            })?;
        let next = manager
            .attribute("topLevelGlobalID")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|next| *next != 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "saved BFGManager topLevelGlobalID is not a nonzero u64",
                )
            })?;
        starting_zoos[0].next_persistent_identifier = Some(
            starting_zoos[0]
                .next_persistent_identifier
                .unwrap_or(next)
                .max(next),
        );
        starting_zoos[0].id = map.starting_zoo;
        starting_zoos[0].map = map.id;
        source.document.starting_zoos = starting_zoos;
        source.scenes = starting.scenes;
        let saved_entity_handles = starting.authored.original_saved_entity_handles;
        source.authored.former_fence_types = starting
            .authored
            .former_fence_types
            .into_iter()
            .map(|((_, persistent), former)| ((map.starting_zoo, persistent), former))
            .collect();
        let (configuration, authored, unlimited_cash, economy) =
            load_saved_world_configuration(&document, map.id, &self.archives, context).await?;
        source.authored.merge(authored)?;
        source.document.scenarios = configuration.scenarios;
        source.document.scenario_script_bindings = configuration.scenario_script_bindings;
        let terrain = context.load::<TerrainAsset>(path);
        let terrain_id = map.terrain;
        let scenario =
            build_restored_world_scenario_asset(&self.server, source, terrain_id, terrain.clone())?;
        let scenario = context
            .add_loaded_labeled_asset("scenario", LoadedAsset::new_with_dependencies(scenario));
        Ok(OriginalSavedGameAsset {
            document,
            scenario,
            terrain,
            economy,
            unlimited_cash,
            saved_entity_handles,
        })
    }
}

pub struct OriginalSavedGameAssetPlugin;

impl Plugin for OriginalSavedGameAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<OriginalSavedGameAsset>()
            .init_asset_loader::<OriginalSavedGameAssetLoader>();
    }
}
