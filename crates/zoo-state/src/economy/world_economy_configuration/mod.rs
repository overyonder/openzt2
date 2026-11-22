use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_refresh_and_indexing::saved_world_economy_asset::SavedWorldEconomyAsset;

#[derive(Component, Clone)]
pub struct WorldEconomyConfiguration(pub Handle<SavedWorldEconomyAsset>);
