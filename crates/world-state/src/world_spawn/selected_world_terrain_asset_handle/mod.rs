use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

/// The strong typed terrain handle selected for the active world root.
#[derive(Component, Debug, Clone)]
pub struct SelectedWorldTerrainAssetHandle(pub Handle<TerrainAsset>);
