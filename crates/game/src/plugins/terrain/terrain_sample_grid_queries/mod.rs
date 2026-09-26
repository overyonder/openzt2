use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterDepth;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_edit_data_types::TerrainSample,
};

pub(super) fn read_terrain_sample_height_centimetres(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    x: usize,
    z: usize,
) -> Option<i16> {
    let local = calculate_row_major_terrain_sample_index(chunk.side, x, z)?;
    edited
        .and_then(|value| value.heights_cm.get(local).copied())
        .or_else(|| {
            i16::try_from(
                (terrain_sample_height_metres(chunk, asset, None, x, z)? * 100.0).round() as i32,
            )
            .ok()
        })
}

pub(super) fn read_materialized_terrain_sample(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    x: usize,
    z: usize,
) -> Option<TerrainSample> {
    let local = calculate_row_major_terrain_sample_index(chunk.side, x, z)?;
    if let Some(value) = edited {
        return Some(TerrainSample {
            height_cm: *value.heights_cm.get(local)?,
            blend: value
                .blend_weights
                .get(local * 16..(local + 1) * 16)?
                .try_into()
                .ok()?,
            ground_cover: *value.ground_cover.get(local)?,
            water_style: *value.water_styles.get(local)?,
            water_biome_channel: *value.water_biome_channels.get(local)?,
            water_cm: *value.water_cm.get(local)?,
        });
    }
    let source = read_authored_terrain_sample(chunk, asset, x, z)?;
    let mut blend = [0; 16];
    *blend.get_mut(usize::from(source.biome_index))? = u8::MAX;
    let height_cm = read_terrain_sample_height_centimetres(chunk, asset, None, x, z)?;
    let water_cm =
        authored_terrain_sample_water_height_metres(asset, source, f32::from(height_cm) * 0.01)
            .and_then(|height| i16::try_from((height * 100.0).round() as i32).ok())
            .unwrap_or(height_cm);
    Some(TerrainSample {
        height_cm,
        blend,
        ground_cover: u8::from(source.has_ground_cover),
        water_style: match source.water_depth {
            TerrainWaterDepth::Dry => 0,
            TerrainWaterDepth::Shallow => 1,
            TerrainWaterDepth::Deep => 2,
        },
        water_cm,
        water_biome_channel: source.biome_index,
    })
}

pub(super) fn calculate_row_major_terrain_sample_index(
    side: u16,
    x: usize,
    z: usize,
) -> Option<usize> {
    (x < side as usize && z < side as usize).then(|| z * side as usize + x)
}

pub(super) fn read_authored_terrain_sample<'a>(
    chunk: &TerrainChunk,
    asset: &'a TerrainAsset,
    x: usize,
    z: usize,
) -> Option<&'a openzt2_game_data::terrain::TerrainSample> {
    asset
        .canonical_terrain_grid()
        .samples
        .get(authored_terrain_sample_index(chunk, asset, x, z)?)
}

pub(super) fn authored_terrain_sample_index(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    x: usize,
    z: usize,
) -> Option<usize> {
    calculate_row_major_terrain_sample_index(chunk.side, x, z)?;
    let cells = usize::from(chunk.side.checked_sub(1)?);
    let gx = usize::try_from(chunk.source_coord.x)
        .ok()?
        .checked_mul(cells)?
        .checked_add(x)?;
    let gz = usize::try_from(chunk.source_coord.y)
        .ok()?
        .checked_mul(cells)?
        .checked_add(cells.checked_sub(z)?)?;
    let grid = asset.canonical_terrain_grid();
    (gx < grid.width as usize && gz < grid.height as usize).then_some(())?;
    gz.checked_mul(grid.width as usize)?.checked_add(gx)
}

pub(super) fn terrain_sample_height_metres(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    x: usize,
    z: usize,
) -> Option<f32> {
    let local = calculate_row_major_terrain_sample_index(chunk.side, x, z)?;
    edited
        .and_then(|value| value.heights_cm.get(local))
        .map(|height| f32::from(*height) * 0.01)
        .or_else(|| {
            read_authored_terrain_sample(chunk, asset, x, z).map(|sample| {
                sample.height_relative_to_base_metres
                    + asset.canonical_terrain_grid().units.base_height_metres
            })
        })
}

pub(super) fn authored_terrain_sample_water_height_metres(
    asset: &TerrainAsset,
    sample: &openzt2_game_data::terrain::TerrainSample,
    height: f32,
) -> Option<f32> {
    let biome = (sample.water_depth != TerrainWaterDepth::Dry).then(|| {
        asset
            .canonical_terrain_grid()
            .biomes
            .get(usize::from(sample.biome_index))
    })??;
    let depth = match sample.water_depth {
        TerrainWaterDepth::Dry => None,
        TerrainWaterDepth::Shallow => biome.shallow_water_depth_mm,
        TerrainWaterDepth::Deep => biome.deep_water_depth_mm.or(biome.shallow_water_depth_mm),
    }?;
    Some(height + depth as f32 * 0.001)
}

pub(super) fn terrain_cell_uses_alternate_triangle_diagonal(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    cell: UVec2,
) -> Option<bool> {
    let cells = u32::from(chunk.side.checked_sub(1)?);
    let x = u32::try_from(chunk.source_coord.x)
        .ok()?
        .checked_mul(cells)?
        .checked_add(cell.x)?;
    let z = u32::try_from(chunk.source_coord.y)
        .ok()?
        .checked_mul(cells)?
        .checked_add(cells.checked_sub(cell.y.checked_add(1)?)?)?;
    let tile_columns = asset
        .canonical_terrain_grid()
        .width
        .checked_sub(1)?
        .checked_div(4)?;
    let tile = z
        .checked_div(4)?
        .checked_mul(tile_columns)?
        .checked_add(x.checked_div(4)?)?;
    asset
        .canonical_terrain_grid()
        .slope_tiles
        .get(tile as usize)
        .map(|tile| !tile.uses_alternate_diagonal)
}
