use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_edit_data_types::TerrainSample,
    terrain_sample_grid_queries::read_materialized_terrain_sample,
};
pub(super) fn materialize_complete_edited_terrain_sample_overlay(
    terrain_chunk: &TerrainChunk,
    terrain_asset: &TerrainAsset,
) -> EditedTerrainSamples {
    let count = terrain_chunk.side as usize * terrain_chunk.side as usize;
    let mut heights = Vec::with_capacity(count);
    let mut blend_weights = Vec::with_capacity(count * 16);
    let mut ground_cover = Vec::with_capacity(count);
    let mut water_styles = Vec::with_capacity(count);
    let mut water_biome_channels = Vec::with_capacity(count);
    let mut water = Vec::with_capacity(count);
    for terrain_sample_z in 0..terrain_chunk.side as usize {
        for terrain_sample_x in 0..terrain_chunk.side as usize {
            let sample = read_materialized_terrain_sample(
                terrain_chunk,
                terrain_asset,
                None,
                terrain_sample_x,
                terrain_sample_z,
            )
            .expect("base sample was validated before overlay materialization");
            heights.push(sample.height_cm);
            blend_weights.extend_from_slice(&sample.blend);
            ground_cover.push(sample.ground_cover);
            water_styles.push(sample.water_style);
            water_biome_channels.push(sample.water_biome_channel);
            water.push(sample.water_cm);
        }
    }
    EditedTerrainSamples {
        heights_cm: heights.into_boxed_slice(),
        blend_weights: blend_weights.into_boxed_slice(),
        ground_cover: ground_cover.into_boxed_slice(),
        water_styles: water_styles.into_boxed_slice(),
        water_biome_channels: water_biome_channels.into_boxed_slice(),
        water_cm: water.into_boxed_slice(),
    }
}

pub(super) fn apply_terrain_sample_rectangle(
    overlay: &mut EditedTerrainSamples,
    side: u16,
    min: UVec2,
    max: UVec2,
    samples: &[TerrainSample],
) {
    let mut source = 0;
    for z in min.y as usize..=max.y as usize {
        for x in min.x as usize..=max.x as usize {
            let destination = z * side as usize + x;
            let sample = samples[source];
            overlay.heights_cm[destination] = sample.height_cm;
            overlay.blend_weights[destination * 16..destination * 16 + 16]
                .copy_from_slice(&sample.blend);
            overlay.ground_cover[destination] = sample.ground_cover;
            overlay.water_styles[destination] = sample.water_style;
            overlay.water_biome_channels[destination] = sample.water_biome_channel;
            overlay.water_cm[destination] = sample.water_cm;
            source += 1;
        }
    }
}

pub(super) fn terrain_sample_rectangle_length(min: UVec2, max: UVec2) -> Option<usize> {
    let width = max.x.checked_sub(min.x)?.checked_add(1)? as usize;
    let height = max.y.checked_sub(min.y)?.checked_add(1)? as usize;
    width.checked_mul(height)
}

pub(super) fn terrain_sample_overlay_has_expected_lengths(
    samples: &EditedTerrainSamples,
    expected: usize,
) -> bool {
    samples.heights_cm.len() == expected
        && samples.blend_weights.len() == expected.saturating_mul(16)
        && samples.ground_cover.len() == expected
        && samples.water_styles.len() == expected
        && samples.water_biome_channels.len() == expected
        && samples.water_cm.len() == expected
}
