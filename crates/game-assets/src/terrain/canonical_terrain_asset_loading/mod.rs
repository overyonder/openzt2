//! Loading a persisted canonical grid through the terrain dependency owner.

use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    gltf::GltfLoader,
    prelude::*,
};
use openzt2_game_data::terrain::TerrainGrid;

use super::{
    source::canonical_terrain_grid_mesh_construction::create_canonical_terrain_grid_glb,
    terrain_asset_dependency_construction::build_terrain_asset_dependencies,
    terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use crate::{
    asset_source::AssetArchives,
    model::bevy_gltf_loader_configuration::create_bevy_gltf_loader_for_lowered_source_assets,
};

#[derive(TypePath)]
pub(super) struct CanonicalTerrainAssetLoader {
    gltf_loader: GltfLoader,
    archives: AssetArchives,
}

impl FromWorld for CanonicalTerrainAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
            gltf_loader: create_bevy_gltf_loader_for_lowered_source_assets(world),
        }
    }
}

impl AssetLoader for CanonicalTerrainAssetLoader {
    type Asset = TerrainAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<TerrainAsset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let (grid, remaining) = postcard::take_from_bytes::<TerrainGrid>(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if !remaining.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "trailing terrain data",
            ));
        }
        let glb = create_canonical_terrain_grid_glb(&grid).map_err(io::Error::other)?;
        build_terrain_asset_dependencies(grid, &glb, &self.gltf_loader, &self.archives, context)
            .await
    }

    fn extensions(&self) -> &[&str] {
        &["ozterrain"]
    }
}
