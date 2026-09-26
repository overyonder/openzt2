use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

/// The strong typed terrain handle selected for the active world root.
#[derive(Component, Debug, Clone)]
pub(crate) struct SelectedWorldTerrainAssetHandle(pub(crate) Handle<TerrainAsset>);
