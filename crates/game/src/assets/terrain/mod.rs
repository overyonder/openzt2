//! Terrain asset registration.

pub(in crate::assets) mod source;

pub mod terrain_asset_types_and_borrowing_queries;
mod terrain_source_asset_loading;

use bevy::prelude::*;

use terrain_asset_types_and_borrowing_queries::TerrainAsset;
use terrain_source_asset_loading::TerrainSourceAssetLoader;

#[derive(Default)]
pub struct TerrainAssetPlugin;

impl Plugin for TerrainAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<TerrainAsset>()
            .preregister_asset_loader::<TerrainSourceAssetLoader>(&["dat"]);
    }

    fn finish(&self, application: &mut App) {
        let terrain_source_asset_loader =
            TerrainSourceAssetLoader::from_world(application.world_mut());
        application.register_asset_loader(terrain_source_asset_loader);
    }
}
