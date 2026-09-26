use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterDepth;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    super::{
        terrain_chunk_types::TerrainChunk,
        terrain_sample_grid_queries::authored_terrain_sample_index,
    },
    TerrainDrySurfaceEligibility,
};

impl TerrainDrySurfaceEligibility<'_, '_> {
    pub(super) fn on_land_in_unchanged_water_region(
        &self,
        chunk: &TerrainChunk,
        asset: &TerrainAsset,
        minimum_x: usize,
        minimum_z: usize,
        world: Vec2,
    ) -> Option<bool> {
        if !self.has_unconstrained_terrain_height_support()? {
            return None;
        }
        // Each cell in the queried tile can attach to its neighboring tiles.
        // Include those complete buckets: three four-interval tiles per axis.
        // Keeping the ring inside this unedited chunk also covers edit seams.
        let low = UVec2::new(
            minimum_x.checked_sub(4)? as u32,
            minimum_z.checked_sub(4)? as u32,
        );
        let high = UVec2::new((minimum_x + 8) as u32, (minimum_z + 8) as u32);
        if high.max_element() >= u32::from(chunk.side) {
            return None;
        }
        let region_index = asset.sample_authored_water_region_index(
            authored_terrain_sample_index(chunk, asset, minimum_x, minimum_z)?,
        )??;
        let grid = asset.canonical_terrain_grid();
        let region = grid.water_regions.get(region_index)?;
        if region.uses_tank_minimum_depth {
            return None;
        }
        let biome = grid.biomes.get(region.water_style_index as usize)?;
        let height = grid.units.base_height_metres + region.height_relative_to_base_metres;
        let source_interval = |millimetres: Option<i32>| {
            millimetres.map_or([0.0, 0.0], |value| {
                [
                    (f64::from(value) - 0.5) * 0.001,
                    (f64::from(value) + 0.5) * 0.001,
                ]
            })
        };
        for candidate in source_interval(biome.deep_water_depth_mm)
            .map(|depth| f64::from(height) - depth)
            .into_iter()
            .chain(
                source_interval(biome.water_shore_offset_mm)
                    .map(|offset| f64::from(height) + offset),
            )
        {
            if !(-1000.0..=1000.0).contains(&candidate) {
                return None;
            }
        }
        for z in low.y..=high.y {
            for x in low.x..=high.x {
                let index = authored_terrain_sample_index(chunk, asset, x as usize, z as usize)?;
                if asset.sample_authored_water_region_index(index)? != Some(region_index) {
                    return None;
                }
                let sample = grid.samples.get(index)?;
                if sample.height_is_linked
                    || sample.has_child_height_link
                    || sample.excludes_elevated_path
                {
                    return None;
                }
            }
        }
        let source = (world - self.index.origin) * Vec2::new(1.0, -1.0);
        let sample_axis = |coordinate: f32, count: u32| {
            let last = usize::try_from(count.checked_sub(1)?).ok()?;
            let cell = (coordinate / chunk.spacing_m)
                .floor()
                .clamp(0.0, last as f32) as usize;
            Some(
                (cell
                    + usize::from(
                        coordinate - cell as f32 * chunk.spacing_m > chunk.spacing_m * 0.5,
                    ))
                .min(last),
            )
        };
        let x = sample_axis(source.x, grid.width)?;
        let z = sample_axis(source.y, grid.height)?;
        let sample = grid
            .samples
            .get(z.checked_mul(grid.width as usize)?.checked_add(x)?)?;
        Some(
            sample.water_depth == TerrainWaterDepth::Shallow
                && grid.units.base_height_metres + sample.height_relative_to_base_metres >= height,
        )
    }
}
