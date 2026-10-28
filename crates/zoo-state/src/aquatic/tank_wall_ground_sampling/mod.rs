//! Ground heights under tank walls, read from canonical terrain at topology
//! node cells.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainUnits;

use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
use crate::topology::topology_graph_types::TopologyGrid;

#[derive(SystemParam)]
pub struct TankWallGroundSampling<'w, 's> {
    grid: Res<'w, TopologyGrid>,
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
}

impl TankWallGroundSampling<'_, '_> {
    pub fn ground_height_at_topology_cell(&self, cell: IVec3) -> Option<f32> {
        let world = self.grid.cell_translation(cell).xz();
        let chunk = terrain_chunk_at(&self.terrain_index, world)?;
        let (chunk, edited) = self.terrain_chunks.get(chunk).ok()?;
        let asset = self.terrain_assets.get(&chunk.asset)?;
        sample_terrain(chunk, asset, edited, world).map(|point| point.height_m)
    }

    /// The terrain's authored units at a topology cell.
    pub fn terrain_units_at_topology_cell(&self, cell: IVec3) -> Option<TerrainUnits> {
        let world = self.grid.cell_translation(cell).xz();
        let chunk = terrain_chunk_at(&self.terrain_index, world)?;
        let (chunk, _) = self.terrain_chunks.get(chunk).ok()?;
        let asset = self.terrain_assets.get(&chunk.asset)?;
        Some(asset.canonical_terrain_grid().units)
    }
}
