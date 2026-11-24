use bevy::prelude::*;

use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_change_tracking_types::{TerrainDirtyRegion, TerrainRenderPending};
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainEditDelta;

pub(super) fn summarize_terrain_edit_sample_changes(
    delta: &TerrainEditDelta,
) -> TerrainDirtyRegion {
    let width = (delta.max.x - delta.min.x + 1) as usize;
    let mut min = UVec2::MAX;
    let mut max = UVec2::ZERO;
    let mut flags = TerrainDirtyFlags::default();
    for (index, (before, after)) in delta.before.iter().zip(delta.after.iter()).enumerate() {
        let mut changed = TerrainDirtyFlags::default();
        if before.height_cm != after.height_cm {
            changed |= TerrainDirtyFlags::HEIGHT;
        }
        if before.blend != after.blend || before.ground_cover != after.ground_cover {
            changed |= TerrainDirtyFlags::SURFACE;
        }
        if before.water_style != after.water_style
            || before.water_biome_channel != after.water_biome_channel
            || before.water_cm != after.water_cm
            || before.water_affiliation != after.water_affiliation
        {
            changed |= TerrainDirtyFlags::WATER;
        }
        if !changed.is_empty() {
            let point = delta.min + UVec2::new((index % width) as u32, (index / width) as u32);
            min = min.min(point);
            max = max.max(point);
            flags |= changed;
        }
    }
    if flags.is_empty() {
        min = delta.min;
        max = delta.max;
    }
    TerrainDirtyRegion { min, max, flags }
}

#[derive(Message)]
pub(super) struct TerrainRenderInvalidated {
    pub(super) chunk: Entity,
    pub(super) region: TerrainDirtyRegion,
}

pub(super) fn merge_terrain_render_invalidations(
    mut commands: Commands,
    mut changes: MessageReader<TerrainRenderInvalidated>,
    mut pending: Query<&mut TerrainRenderPending>,
) {
    let mut dirty = bevy::ecs::entity::EntityHashMap::default();
    for change in changes.read() {
        dirty
            .entry(change.chunk)
            .and_modify(|region| {
                *region = merge_terrain_dirty_sample_rectangles(*region, change.region)
            })
            .or_insert(change.region);
    }
    for (chunk, region) in dirty {
        if let Ok(mut pending) = pending.get_mut(chunk) {
            let merged = merge_terrain_dirty_sample_rectangles(pending.0, region);
            pending.set_if_neq(TerrainRenderPending(merged));
        } else {
            commands
                .entity(chunk)
                .try_insert(TerrainRenderPending(region));
        }
    }
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
