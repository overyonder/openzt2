use bevy::prelude::*;

use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_change_tracking_types::{TerrainDirtyRegion, TerrainRenderPending};
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainSample;

pub(super) fn determine_terrain_dirty_flags_from_changed_samples(
    terrain_samples_before_change: &[TerrainSample],
    terrain_samples_after_change: &[TerrainSample],
) -> TerrainDirtyFlags {
    terrain_samples_before_change
        .iter()
        .zip(terrain_samples_after_change)
        .fold(
            TerrainDirtyFlags::default(),
            |mut changed_terrain_properties, (sample_before_change, sample_after_change)| {
                if sample_before_change.height_cm != sample_after_change.height_cm {
                    changed_terrain_properties |=
                        TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::COLLISION;
                }
                if sample_before_change.blend != sample_after_change.blend
                    || sample_before_change.ground_cover != sample_after_change.ground_cover
                {
                    changed_terrain_properties |= TerrainDirtyFlags::SURFACE;
                }
                if sample_before_change.water_style != sample_after_change.water_style
                    || sample_before_change.water_biome_channel
                        != sample_after_change.water_biome_channel
                    || sample_before_change.water_cm != sample_after_change.water_cm
                    || sample_before_change.water_affiliation
                        != sample_after_change.water_affiliation
                {
                    changed_terrain_properties |=
                        TerrainDirtyFlags::WATER | TerrainDirtyFlags::COLLISION;
                }
                changed_terrain_properties
            },
        )
}

/// Inclusive bounds of the samples that differ between `before` and `after`,
/// which cover the row-major rectangle from `min` to `max`.
pub(super) fn changed_terrain_sample_bounds(
    before: &[TerrainSample],
    after: &[TerrainSample],
    min: UVec2,
    max: UVec2,
) -> Option<(UVec2, UVec2)> {
    let width = (max.x - min.x + 1) as usize;
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(index, _)| min + UVec2::new((index % width) as u32, (index / width) as u32))
        .fold(None, |bounds, sample| match bounds {
            None => Some((sample, sample)),
            Some((low, high)) => Some((low.min(sample), high.max(sample))),
        })
}

pub(super) fn queue_terrain_render_refresh(
    commands: &mut Commands,
    chunk: Entity,
    region: TerrainDirtyRegion,
) {
    commands
        .entity(chunk)
        .entry::<TerrainRenderPending>()
        .and_modify(move |mut pending| {
            pending.0 = merge_terrain_dirty_sample_rectangles(pending.0, region);
        })
        .or_insert(TerrainRenderPending(region));
}

pub(super) fn merge_terrain_dirty_sample_rectangles(
    pending: TerrainDirtyRegion,
    next: TerrainDirtyRegion,
) -> TerrainDirtyRegion {
    TerrainDirtyRegion {
        min: pending.min.min(next.min),
        max: pending.max.max(next.max),
        flags: pending.flags | next.flags,
    }
}

pub(super) fn blend_terrain_biome_channel_weights_toward_full_strength(
    biome_channel_weights: &mut [u8; 16],
    painted_biome_channel_index: usize,
    brush_strength: f32,
) {
    if painted_biome_channel_index >= biome_channel_weights.len() || brush_strength <= 0.0 {
        return;
    }
    let painted_biome_channel_weight =
        (f32::from(biome_channel_weights[painted_biome_channel_index])
            + (255.0 - f32::from(biome_channel_weights[painted_biome_channel_index]))
                * brush_strength)
            .round()
            .clamp(0.0, 255.0) as u8;
    let remaining_weight = 255_u16.saturating_sub(u16::from(painted_biome_channel_weight));
    let other_biome_channel_total = biome_channel_weights
        .iter()
        .enumerate()
        .filter(|(biome_channel_index, _)| *biome_channel_index != painted_biome_channel_index)
        .map(|(_, biome_channel_weight)| u16::from(*biome_channel_weight))
        .sum::<u16>();
    biome_channel_weights
        .iter_mut()
        .enumerate()
        .filter(|(biome_channel_index, _)| *biome_channel_index != painted_biome_channel_index)
        .for_each(|(_, biome_channel_weight)| {
            *biome_channel_weight = if other_biome_channel_total == 0 {
                0
            } else {
                ((u32::from(*biome_channel_weight) * u32::from(remaining_weight)
                    + u32::from(other_biome_channel_total / 2))
                    / u32::from(other_biome_channel_total)) as u8
            };
        });
    biome_channel_weights[painted_biome_channel_index] = painted_biome_channel_weight;
    let normalized_weight_total = biome_channel_weights
        .iter()
        .map(|biome_channel_weight| u16::from(*biome_channel_weight))
        .sum::<u16>();
    if normalized_weight_total != 255 {
        biome_channel_weights[painted_biome_channel_index] =
            i32::from(biome_channel_weights[painted_biome_channel_index])
                .saturating_add(255_i32.saturating_sub(i32::from(normalized_weight_total)))
                .clamp(0, 255) as u8;
    }
}
