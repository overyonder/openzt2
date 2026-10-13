use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    super::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
        terrain_edit_rectangle_operations::terrain_sample_overlay_has_expected_lengths,
        terrain_sample_grid_queries::authored_terrain_sample_index,
        terrain_world_sampling::sample_terrain,
    },
    TerrainDrySurfaceEligibility,
};

impl TerrainDrySurfaceEligibility<'_, '_> {
    pub(super) fn on_land_with_edited_water(
        &self,
        chunk: &TerrainChunk,
        asset: &TerrainAsset,
        edited: &EditedTerrainSamples,
        minimum_x: usize,
        minimum_z: usize,
        world: Vec2,
    ) -> Option<bool> {
        if !self.has_unconstrained_terrain_height_support()?
            || !terrain_sample_overlay_has_expected_lengths(edited, usize::from(chunk.side).pow(2))
        {
            return None;
        }
        // Keep the neighboring attachment tiles inside this chunk. Edited
        // samples do not yet represent linked heights or tank side constraints.
        let low_x = minimum_x.checked_sub(4)?;
        let low_z = minimum_z.checked_sub(4)?;
        let high_x = minimum_x + 8;
        let high_z = minimum_z + 8;
        if high_x.max(high_z) >= usize::from(chunk.side) {
            return None;
        }
        let grid = asset.canonical_terrain_grid();
        for z in low_z..=high_z {
            for x in low_x..=high_x {
                let index = authored_terrain_sample_index(chunk, asset, x, z)?;
                let sample = grid.samples.get(index)?;
                if sample.height_is_linked
                    || sample.has_child_height_link
                    || sample.excludes_elevated_path
                {
                    return None;
                }
                if let Some(region) = asset.sample_authored_water_region_index(index)? {
                    if grid.water_regions.get(region)?.uses_tank_minimum_depth {
                        return None;
                    }
                }
            }
        }
        // The overlay supersedes saved wet flags and region membership. Use
        // the same interpolated ground and water surface as terrain consumers.
        let surface = sample_terrain(chunk, asset, Some(edited), world)?;
        Some(surface.water_height_m.is_none())
    }
}
