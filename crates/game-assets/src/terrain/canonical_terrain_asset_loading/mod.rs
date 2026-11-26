use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainGrid;
use std::path::PathBuf;

use crate::asset_source::{generated_asset_publication::GeneratedAssetPublication, AssetArchives};

use super::{
    terrain_asset_dependency_construction::build_terrain_asset_dependencies,
    terrain_asset_types_and_borrowing_queries::TerrainAsset,
};

pub fn add_restored_terrain_asset(
    server: &AssetServer,
    archives: &AssetArchives,
    grid: TerrainGrid,
    declaring_path: PathBuf,
) -> Handle<TerrainAsset> {
    let server_for_assets = server.clone();
    let archives = archives.clone();
    server.add_async(async move {
        let mut publication = GeneratedAssetPublication::Runtime {
            server: &server_for_assets,
            source: &declaring_path,
        };
        build_terrain_asset_dependencies(grid, &archives, &mut publication).await
    })
}
