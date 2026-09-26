use bevy::prelude::*;

use super::{
    terrain_change_tracking_types::{TerrainDirty, TerrainDirtyFlags},
    terrain_edit_data_types::TerrainSample,
};

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
                {
                    changed_terrain_properties |=
                        TerrainDirtyFlags::WATER | TerrainDirtyFlags::COLLISION;
                }
                changed_terrain_properties
            },
        )
}

pub(super) fn merge_terrain_dirty_sample_rectangle_and_advance_revision(
    current: Option<TerrainDirty>,
    min: UVec2,
    max: UVec2,
    flags: TerrainDirtyFlags,
) -> TerrainDirty {
    match current {
        Some(current) => TerrainDirty {
            min: current.min.min(min),
            max: current.max.max(max),
            flags: current.flags | flags,
            revision: current.revision.checked_add(1).unwrap_or(u64::MAX),
        },
        None => TerrainDirty {
            min,
            max,
            flags,
            revision: 1,
        },
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
