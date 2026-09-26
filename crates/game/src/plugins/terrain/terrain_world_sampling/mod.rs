use bevy::prelude::*;
use openzt2_game_data::{terrain::TerrainWaterDepth, AssetId};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use super::terrain_sample_grid_queries::{
    authored_terrain_sample_water_height_metres, calculate_row_major_terrain_sample_index,
    read_authored_terrain_sample, terrain_cell_uses_alternate_triangle_diagonal,
    terrain_sample_height_metres,
};
use super::terrain_world_sampling_types::{
    AuthoredTerrainPoint, TerrainBiomePoint, TerrainPoint, TerrainWaterPoint,
};

pub(crate) fn terrain_chunk_at(index: &TerrainIndex, world: Vec2) -> Option<Entity> {
    (index.chunk_span_m > 0.0 && world.is_finite())
        .then(|| {
            ((world - index.origin) / index.chunk_span_m)
                .floor()
                .as_ivec2()
        })
        .and_then(|coord| index.chunks.get(&coord).copied())
}

pub(crate) fn terrain_cell_indices(chunk: &TerrainChunk, world: Vec2) -> Option<UVec2> {
    let grid = (world - chunk.origin) / chunk.spacing_m;
    let max = f32::from(chunk.side.checked_sub(1)?);
    (chunk.spacing_m > 0.0
        && grid.is_finite()
        && grid.cmpge(Vec2::ZERO).all()
        && grid.cmple(Vec2::splat(max)).all())
    .then(|| {
        UVec2::new(
            (grid.x.floor() as u32).min(chunk.side as u32 - 2),
            (grid.y.floor() as u32).min(chunk.side as u32 - 2),
        )
    })
}

#[allow(clippy::many_single_char_names)] // Compact corner notation makes the two triangle normals directly comparable.
pub(crate) fn terrain_cell_averaged_normal(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    cell: UVec2,
) -> Option<Vec3> {
    let x = cell.x as usize;
    let z = cell.y as usize;
    let h = |x, z| terrain_sample_height_metres(chunk, asset, edited, x, z);
    let (a, b, c, d) = (h(x, z)?, h(x + 1, z)?, h(x, z + 1)?, h(x + 1, z + 1)?);
    let alternate = terrain_cell_uses_alternate_triangle_diagonal(chunk, asset, cell)?;
    let normal = if alternate {
        Vec3::new(a - b, chunk.spacing_m, a - c) + Vec3::new(c - d, chunk.spacing_m, b - d)
    } else {
        Vec3::new(c - d, chunk.spacing_m, a - c) + Vec3::new(a - b, chunk.spacing_m, b - d)
    }
    .normalize_or_zero();
    (normal != Vec3::ZERO).then_some(normal)
}

pub(crate) fn sample_terrain(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<TerrainPoint> {
    let cell = terrain_cell_indices(chunk, world)?;
    let grid = (world - chunk.origin) / chunk.spacing_m;
    let x = cell.x as usize;
    let z = cell.y as usize;
    let tx = grid.x - x as f32;
    let tz = grid.y - z as f32;
    let h = |x, z| terrain_sample_height_metres(chunk, asset, edited, x, z);
    let (h00, h10, h01, h11) = (h(x, z)?, h(x + 1, z)?, h(x, z + 1)?, h(x + 1, z + 1)?);
    let alternate = terrain_cell_uses_alternate_triangle_diagonal(chunk, asset, cell)?;
    let (height_m, normal) = triangles(h00, h10, h01, h11, chunk.spacing_m, tx, tz, alternate);
    let nx = if tx < 0.5 { x } else { x + 1 };
    let nz = if tz < 0.5 { z } else { z + 1 };
    let local = calculate_row_major_terrain_sample_index(chunk.side, nx, nz)?;
    let source = read_authored_terrain_sample(chunk, asset, nx, nz)?;
    let surface = edited
        .and_then(|value| dominant_edited_terrain_biome_channel(value, local))
        .unwrap_or(u32::from(source.biome_index));
    let water_height_m = if let Some(edited) = edited {
        (edited.water_styles.get(local).copied()? != 0)
            .then_some(f32::from(*edited.water_cm.get(local)?) * 0.01)
            .filter(|water| *water > height_m)
    } else {
        authored_terrain_sample_water_height_metres(asset, source, height_m)
    };
    Some(TerrainPoint {
        height_m,
        water_height_m,
        surface,
        normal,
    })
}

pub(crate) fn sample_authored_terrain(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<AuthoredTerrainPoint> {
    let geometry = sample_terrain(chunk, asset, edited, world)?;
    let grid = ((world - chunk.origin) / chunk.spacing_m)
        .round()
        .clamp(Vec2::ZERO, Vec2::splat(f32::from(chunk.side - 1)));
    let local =
        calculate_row_major_terrain_sample_index(chunk.side, grid.x as usize, grid.y as usize)?;
    let source = read_authored_terrain_sample(chunk, asset, grid.x as usize, grid.y as usize)?;
    let (channel, weight, biome_id) = edited
        .and_then(|edited| {
            let weights = edited.blend_weights.get(local * 16..(local + 1) * 16)?;
            let (channel, weight) = weights
                .iter()
                .copied()
                .enumerate()
                .max_by_key(|(channel, weight)| (*weight, std::cmp::Reverse(*channel)))?;
            Some((
                channel as u8,
                weight,
                AssetId::from_key(&asset.canonical_terrain_grid().biomes.get(channel)?.name),
            ))
        })
        .unwrap_or((
            source.biome_index,
            u8::MAX,
            AssetId::from_key(
                &asset
                    .canonical_terrain_grid()
                    .biomes
                    .get(usize::from(source.biome_index))?
                    .name,
            ),
        ));
    let water_biome_channel = edited
        .and_then(|edited| edited.water_biome_channels.get(local).copied())
        .unwrap_or(source.biome_index);
    let water_biome = asset
        .canonical_terrain_grid()
        .biomes
        .get(usize::from(water_biome_channel))?;
    let water_style = edited
        .and_then(|edited| edited.water_styles.get(local).copied())
        .unwrap_or(match source.water_depth {
            TerrainWaterDepth::Dry => 0,
            TerrainWaterDepth::Shallow => 1,
            TerrainWaterDepth::Deep => 2,
        });
    let water = geometry
        .water_height_m
        .and_then(|_| water_biome.water_presentation.as_ref())
        .map(|water_presentation| TerrainWaterPoint {
            biome_channel: water_biome_channel,
            style: water_style,
            id: AssetId::from_key(&format!("{}/water/{water_style}", water_biome.name)),
            appearance: AssetId::from_key(&water_presentation.texture_animations[0].texture),
            flags: 0,
        });
    Some(AuthoredTerrainPoint {
        geometry,
        biome: Some(TerrainBiomePoint {
            id: biome_id,
            channel,
            weight,
        }),
        water,
    })
}

fn dominant_edited_terrain_biome_channel(
    edited_terrain_samples: &EditedTerrainSamples,
    terrain_sample_index: usize,
) -> Option<u32> {
    edited_terrain_samples
        .blend_weights
        .get(terrain_sample_index * 16..(terrain_sample_index + 1) * 16)?
        .iter()
        .enumerate()
        .max_by_key(|(channel, weight)| (**weight, std::cmp::Reverse(*channel)))
        .map(|(channel, _)| channel as u32)
}

#[allow(clippy::many_single_char_names)] // These are the standard bilinear cell corner and local-coordinate names.
fn triangles(
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    spacing: f32,
    x: f32,
    z: f32,
    alternate: bool,
) -> (f32, Vec3) {
    if alternate {
        if x + z <= 1.0 {
            (
                a + (b - a) * x + (c - a) * z,
                Vec3::new(a - b, spacing, a - c).normalize_or_zero(),
            )
        } else {
            (
                d + (c - d) * (1.0 - x) + (b - d) * (1.0 - z),
                Vec3::new(c - d, spacing, b - d).normalize_or_zero(),
            )
        }
    } else if x <= z {
        (
            a + (c - a) * z + (d - c) * x,
            Vec3::new(c - d, spacing, a - c).normalize_or_zero(),
        )
    } else {
        (
            a + (b - a) * x + (d - b) * z,
            Vec3::new(a - b, spacing, b - d).normalize_or_zero(),
        )
    }
}
