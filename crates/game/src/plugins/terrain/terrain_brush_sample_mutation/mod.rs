use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::AssetId;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_brush_interaction_state::TerrainBrushDab,
    terrain_brush_types::{TerrainBrushFalloff, TerrainBrushKind, TerrainBrushPreview},
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_edit_data_types::{TerrainEditDelta, TerrainSample},
    terrain_edit_rectangle_operations::terrain_sample_rectangle_length,
    terrain_sample_change_calculations::blend_terrain_biome_channel_weights_toward_full_strength,
    terrain_sample_grid_queries::read_materialized_terrain_sample,
};

pub(super) fn resolve_painted_water_surface_height_centimetres<'a>(
    preview: &TerrainBrushPreview,
    dab: &TerrainBrushDab,
    chunks: impl Iterator<
        Item = (
            &'a TerrainChunk,
            &'a TerrainAsset,
            Option<&'a EditedTerrainSamples>,
        ),
    >,
) -> Option<i16> {
    if !matches!(preview.kind, TerrainBrushKind::PaintWater { .. }) {
        return None;
    }
    let mut centre = None;
    let mut sum = 0_i64;
    let mut count = 0_i64;
    let mut existing_plane = None;
    let mut multiple_planes = false;
    for (chunk, asset, edited) in chunks {
        if let Some(point) =
            super::terrain_world_sampling::sample_terrain(chunk, asset, edited, dab.center)
        {
            centre = Some((point.height_m * 100.0).round() as i16);
        }
        for z in 0..usize::from(chunk.side.saturating_sub(1)) {
            for x in 0..usize::from(chunk.side.saturating_sub(1)) {
                let world = chunk.origin + Vec2::new(x as f32, z as f32) * chunk.spacing_m;
                if world.distance_squared(dab.center) > preview.radius_m * preview.radius_m {
                    continue;
                }
                let Some(sample) = read_materialized_terrain_sample(chunk, asset, edited, x, z)
                else {
                    continue;
                };
                sum += i64::from(sample.height_cm);
                count += 1;
                if sample.water_style != 0 && sample.water_cm > sample.height_cm {
                    if existing_plane.is_some_and(|height| height != sample.water_cm) {
                        multiple_planes = true;
                    }
                    existing_plane = Some(sample.water_cm);
                }
            }
        }
    }
    if !multiple_planes && existing_plane.is_some() {
        return existing_plane;
    }
    let centre = centre?;
    let mean = if count == 0 {
        centre
    } else {
        (sum / count) as i16
    };
    Some(if (i32::from(mean) - i32::from(centre)).abs() < 50 {
        mean
    } else {
        centre
    })
}

pub(super) fn prepare_terrain_chunk_sample_delta_from_brush_dabs(
    persistent: TerrainChunkId,
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    preview: &TerrainBrushPreview,
    dabs: &[TerrainBrushDab],
    water_surface_heights_cm: &[Option<i16>],
) -> Option<TerrainEditDelta> {
    let (world_min, world_max) = dabs.iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(minimum, maximum), dab| {
            (
                minimum.min(dab.center - Vec2::splat(preview.radius_m)),
                maximum.max(dab.center + Vec2::splat(preview.radius_m)),
            )
        },
    );
    let local_min = ((world_min - chunk.origin) / chunk.spacing_m).floor();
    let local_max = ((world_max - chunk.origin) / chunk.spacing_m).ceil();
    let min = UVec2::new(local_min.x.max(0.0) as u32, local_min.y.max(0.0) as u32)
        .min(UVec2::splat(chunk.side as u32 - 1));
    let max = UVec2::new(local_max.x.max(0.0) as u32, local_max.y.max(0.0) as u32)
        .min(UVec2::splat(chunk.side as u32 - 1));
    if min.x > max.x || min.y > max.y {
        return None;
    }

    let capacity = terrain_sample_rectangle_length(min, max)?;
    let mut before = Vec::with_capacity(capacity);
    let mut after = Vec::with_capacity(capacity);
    let paint_channels = match preview.kind {
        TerrainBrushKind::Paint {
            biome,
            ground_cover,
            ..
        } => {
            let mut channels = [None; 2];
            for cover in [false, true] {
                if ground_cover.is_none_or(|requested| requested == cover) {
                    channels[usize::from(cover)] = find_terrain_biome_blend_channel(asset, biome);
                }
            }
            channels
        }
        _ => [None; 2],
    };
    let painted_water = match preview.kind {
        TerrainBrushKind::PaintWater { biome, depth } => {
            find_authored_water_depth_for_terrain_biome(asset, biome, depth)
        }
        _ => None,
    };

    let mut any_changed = false;
    for z in min.y as usize..=max.y as usize {
        for x in min.x as usize..=max.x as usize {
            let original = read_materialized_terrain_sample(chunk, asset, edited, x, z)?;
            let add_water_style = matches!(preview.kind, TerrainBrushKind::AddWater)
                .then(|| find_default_water_style_for_terrain_sample(asset, edited, &original))
                .flatten();
            let world = chunk.origin + Vec2::new(x as f32, z as f32) * chunk.spacing_m;
            let result = dabs.iter().zip(water_surface_heights_cm).fold(
                original,
                |current, (dab, water_surface_height_cm)| {
                    let weight = calculate_terrain_brush_weight_with_falloff(
                        world.distance(dab.center),
                        preview.radius_m,
                        preview.falloff,
                    );
                    apply_terrain_brush_to_sample(
                        current,
                        chunk,
                        asset,
                        edited,
                        x,
                        z,
                        preview,
                        dab.seconds,
                        weight,
                        paint_channels,
                        painted_water,
                        add_water_style,
                        *water_surface_height_cm,
                    )
                },
            );
            any_changed |= result != original;
            before.push(original);
            after.push(result);
        }
    }
    any_changed.then(|| TerrainEditDelta {
        chunk: persistent,
        min,
        max,
        before: before.into_boxed_slice(),
        after: after.into_boxed_slice(),
    })
}

fn apply_terrain_brush_to_sample(
    original: TerrainSample,
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    x: usize,
    z: usize,
    preview: &TerrainBrushPreview,
    seconds: f32,
    weight: f32,
    paint_channels: [Option<usize>; 2],
    painted_water: Option<(usize, TerrainWaterDepth, i32)>,
    add_water_style: Option<(u16, u8)>,
    water_surface_height_cm: Option<i16>,
) -> TerrainSample {
    if weight <= 0.0 {
        return original;
    }
    let magnitude = (preview.strength_per_s.abs() * seconds * weight * 100.0).round();
    let height_delta = magnitude.clamp(0.0, f32::from(i16::MAX)) as i16;
    let mut result = original;
    match preview.kind {
        TerrainBrushKind::Raise => result.height_cm = result.height_cm.saturating_add(height_delta),
        TerrainBrushKind::Lower => result.height_cm = result.height_cm.saturating_sub(height_delta),
        TerrainBrushKind::Flatten { height_cm } => {
            let blend = if preview.strength_per_s == 0.0 {
                1.0
            } else {
                (preview.strength_per_s.abs() * seconds * weight).clamp(0.0, 1.0)
            };
            result.height_cm = (f32::from(original.height_cm)
                + (f32::from(height_cm) - f32::from(original.height_cm)) * blend)
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                as i16;
        }
        TerrainBrushKind::Smooth => {
            let mut sum = 0_i32;
            let mut count = 0_i32;
            for nz in z.saturating_sub(1)..=(z + 1).min(chunk.side as usize - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(chunk.side as usize - 1) {
                    if let Some(height) =
                        super::terrain_sample_grid_queries::read_terrain_sample_height_centimetres(
                            chunk, asset, edited, nx, nz,
                        )
                    {
                        sum += i32::from(height);
                        count += 1;
                    }
                }
            }
            if count > 0 {
                let average = sum as f32 / count as f32;
                let blend = (preview.strength_per_s.abs() * seconds * weight).clamp(0.0, 1.0);
                result.height_cm = (f32::from(original.height_cm)
                    + (average - f32::from(original.height_cm)) * blend)
                    .round()
                    .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                    as i16;
            }
        }
        TerrainBrushKind::Paint { ground_cover, .. } => {
            let surface = ground_cover.map_or(usize::from(result.ground_cover != 0), usize::from);
            if let Some(channel) = paint_channels[surface] {
                let strength = (preview.strength_per_s.abs() * seconds * weight).clamp(0.0, 1.0);
                blend_terrain_biome_channel_weights_toward_full_strength(
                    &mut result.blend,
                    channel,
                    strength,
                );
                if let Some(ground_cover) = ground_cover {
                    result.ground_cover = u8::from(ground_cover);
                }
                result.water_style = 0;
                result.water_cm = result.height_cm;
            }
        }
        TerrainBrushKind::PaintWater { .. } => {
            let Some(water_surface_height_cm) = water_surface_height_cm else {
                return original;
            };
            let Some((channel, depth, depth_mm)) = painted_water else {
                return original;
            };
            let strength = (preview.strength_per_s.abs() * seconds * weight).clamp(0.0, 1.0);
            blend_terrain_biome_channel_weights_toward_full_strength(
                &mut result.blend,
                channel,
                strength,
            );
            result.ground_cover = 0;
            result.water_biome_channel = channel as u8;
            result.water_style = match depth {
                TerrainWaterDepth::Dry => 0,
                TerrainWaterDepth::Shallow => 1,
                TerrainWaterDepth::Deep => 2,
            };
            let ground_offset_centimetres = (depth_mm as f32 * 0.1)
                .round()
                .clamp(0.0, f32::from(i16::MAX)) as i16;
            result.water_cm = water_surface_height_cm;
            result.height_cm = water_surface_height_cm.saturating_sub(
                (f32::from(ground_offset_centimetres) * weight.clamp(0.0, 1.0)).round() as i16,
            );
        }
        TerrainBrushKind::AddWater => {
            if result.water_style == 0 {
                let Some((style, channel)) = add_water_style else {
                    return original;
                };
                result.water_style = style;
                result.water_biome_channel = channel;
            }
            result.water_cm = result
                .water_cm
                .max(result.height_cm)
                .saturating_add(height_delta)
        }
        TerrainBrushKind::RemoveWater => {
            result.water_cm = result
                .water_cm
                .saturating_sub(height_delta)
                .max(result.height_cm);
            if result.water_cm <= result.height_cm {
                result.water_style = 0;
            }
        }
    }
    if let Some(source) =
        super::terrain_sample_grid_queries::read_authored_terrain_sample(chunk, asset, x, z)
    {
        if let Some(minimum) = source.support_floor_metres {
            result.height_cm = result.height_cm.max((minimum * 100.0).round() as i16);
        }
        if let Some(maximum) = source.height_cap_metres {
            result.height_cm = result.height_cm.min((maximum * 100.0).round() as i16);
        }
    }
    result
}
pub(super) fn calculate_terrain_brush_weight_with_falloff(
    distance: f32,
    radius: f32,
    falloff: TerrainBrushFalloff,
) -> f32 {
    let normalized = (1.0 - distance / radius).clamp(0.0, 1.0);
    match falloff {
        TerrainBrushFalloff::Constant => (distance <= radius) as u8 as f32,
        TerrainBrushFalloff::Linear => normalized,
        TerrainBrushFalloff::Smooth => normalized * normalized * (3.0 - 2.0 * normalized),
        TerrainBrushFalloff::Cosine => {
            (distance <= radius) as u8 as f32
                * (((distance / radius) * std::f32::consts::PI).cos() + 1.0)
        }
    }
}

pub(super) fn paint_layers_are_available_for_terrain_brush(
    asset: &TerrainAsset,
    _edited: Option<&EditedTerrainSamples>,
    biome: AssetId,
    _ground_cover: Option<bool>,
) -> bool {
    find_terrain_biome_blend_channel(asset, biome).is_some()
}

pub(super) fn painted_water_depth_is_available_for_terrain_brush(
    asset: &TerrainAsset,
    biome: AssetId,
    depth: TerrainWaterDepth,
) -> bool {
    find_authored_water_depth_for_terrain_biome(asset, biome, depth).is_some()
}

fn find_terrain_biome_blend_channel(asset: &TerrainAsset, biome: AssetId) -> Option<usize> {
    asset
        .canonical_terrain_grid()
        .biomes
        .iter()
        .position(|candidate| AssetId::from_key(&candidate.name) == biome)
}

fn find_authored_water_depth_for_terrain_biome(
    asset: &TerrainAsset,
    biome: AssetId,
    depth: TerrainWaterDepth,
) -> Option<(usize, TerrainWaterDepth, i32)> {
    let channel = find_terrain_biome_blend_channel(asset, biome)?;
    let biome = asset.canonical_terrain_grid().biomes.get(channel)?;
    let depth_mm = match depth {
        TerrainWaterDepth::Dry => return None,
        TerrainWaterDepth::Shallow => biome.shallow_water_depth_mm?,
        TerrainWaterDepth::Deep => biome.deep_water_depth_mm?,
    };
    Some((channel, depth, depth_mm))
}

fn find_default_water_style_for_terrain_sample(
    asset: &TerrainAsset,
    _edited: Option<&EditedTerrainSamples>,
    sample: &TerrainSample,
) -> Option<(u16, u8)> {
    let channel = sample
        .blend
        .iter()
        .enumerate()
        .max_by_key(|(channel, weight)| (**weight, std::cmp::Reverse(*channel)))?
        .0 as u8;
    asset
        .canonical_terrain_grid()
        .biomes
        .get(usize::from(channel))
        .filter(|candidate| candidate.water_presentation.is_some())
        .map(|_| (1, channel))
}
