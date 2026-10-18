use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use crate::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};

/// Borrows the same resolved water membership used by rendering and physics.
#[derive(SystemParam)]
pub struct TerrainDrySurfaceEligibility<'w, 's> {
    index: Res<'w, TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    membership: Query<'w, 's, &'static TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery<'w, 's>,
}

impl TerrainDrySurfaceEligibility<'_, '_> {
    pub fn on_land(&self, world: Vec2) -> Option<bool> {
        let entity = terrain_chunk_at(&self.index, world)?;
        let (chunk, edited) = self.chunks.get(entity).ok()?;
        let asset = self.assets.get(&chunk.asset)?;
        let membership = self
            .membership
            .iter()
            .find(|membership| membership.terrain() == chunk.asset.id())?;
        let Some(owner) = membership.owner_at_world_xz(world, asset)? else {
            return Some(true);
        };
        let surface = sample_terrain(chunk, asset, edited, world)?;
        Some(surface.height_m >= self.regions.get(owner)?.surface_height(asset)?)
    }
}
