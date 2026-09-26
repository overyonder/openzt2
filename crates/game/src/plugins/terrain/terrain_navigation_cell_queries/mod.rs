use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_sample_grid_queries,
};

/// Combines the four corner samples into a cell's height and water measurements.
pub(crate) fn read_navigation_cell_height_and_water_from_terrain_samples(
    terrain_chunk: &TerrainChunk,
    terrain_asset: &TerrainAsset,
    edited_terrain_samples: Option<&EditedTerrainSamples>,
    terrain_cell: UVec2,
) -> Option<(i32, bool)> {
    let sample_x = usize::try_from(terrain_cell.x).ok()?;
    let sample_z = usize::try_from(terrain_cell.y).ok()?;
    let terrain_cell_count = usize::from(terrain_chunk.side.checked_sub(1)?);
    (sample_x < terrain_cell_count && sample_z < terrain_cell_count).then_some(())?;
    let corner_samples = [
        terrain_sample_grid_queries::read_materialized_terrain_sample(
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            sample_x,
            sample_z,
        )?,
        terrain_sample_grid_queries::read_materialized_terrain_sample(
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            sample_x + 1,
            sample_z,
        )?,
        terrain_sample_grid_queries::read_materialized_terrain_sample(
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            sample_x,
            sample_z + 1,
        )?,
        terrain_sample_grid_queries::read_materialized_terrain_sample(
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            sample_x + 1,
            sample_z + 1,
        )?,
    ];
    average_terrain_corner_samples_into_navigation_cell_height_and_water(
        corner_samples.map(|sample| (i64::from(sample.height_cm), sample.water_style != 0)),
    )
}

/// Applies the terrain-owned reduction from four corner samples to one
/// navigation cell.
pub(crate) fn average_terrain_corner_samples_into_navigation_cell_height_and_water(
    corner_samples: [(i64, bool); 4],
) -> Option<(i32, bool)> {
    let average_height_centimetres = corner_samples
        .iter()
        .try_fold(0_i64, |height_sum, (height_centimetres, _)| {
            height_sum.checked_add(*height_centimetres)
        })?
        / 4;
    Some((
        i32::try_from(average_height_centimetres).ok()?,
        corner_samples
            .iter()
            .all(|(_, contains_water)| *contains_water),
    ))
}
