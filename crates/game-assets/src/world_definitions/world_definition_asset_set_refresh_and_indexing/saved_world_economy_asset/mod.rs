//! Typed tracker data from the economy configuration selected by a saved zoo.

use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::{
    world_definitions::economy_transactions::EconomyTransactionDefinition, AssetId,
};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
    world_scenario::original_saved_world_configuration_loading::load_saved_world_economy,
};

pub const SAVED_WORLD_ECONOMY_LABEL: &str = "economy-trackers";

#[derive(Asset, TypePath)]
pub struct SavedWorldEconomyAsset {
    pub trackers: Box<[SavedWorldEconomyTrackerDefinition]>,
}

pub struct SavedWorldEconomyTrackerDefinition {
    pub name: AssetId,
    pub transactions: Vec<EconomyTransactionDefinition>,
}

#[derive(TypePath)]
pub struct SavedWorldEconomyAssetLoader(AssetArchives);

impl FromWorld for SavedWorldEconomyAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource::<AssetArchives>().clone())
    }
}

impl AssetLoader for SavedWorldEconomyAssetLoader {
    type Asset = SavedWorldEconomyAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path();
        let source =
            crate::asset_source::archive_entry_asset_path::archive_entry_asset_source_path(path)
                .map(|(path, _, _)| path);
        let document = parse_blue_fang_source_document(
            AssetPath::new(&source.as_deref().unwrap_or(path).to_string_lossy()),
            &bytes,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let (asset, _) = load_saved_world_economy(&document, &self.0, context).await?;
        Ok(asset)
    }

    // Callers select this loader by asset type. Extension defaults stay with
    // the complete saved-game and starting-world loaders.
}
