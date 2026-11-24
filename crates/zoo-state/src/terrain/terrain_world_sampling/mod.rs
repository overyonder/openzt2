use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::AssetId;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::terrain::terrain_chunk_types::TerrainChunk;
use crate::terrain::terrain_chunk_types::TerrainIndex;
use crate::terrain::terrain_sample_grid_queries::calculate_row_major_terrain_sample_index;
use crate::terrain::terrain_sample_grid_queries::terrain_cell_uses_alternate_triangle_diagonal;
use crate::terrain::terrain_sample_grid_queries::terrain_sample_height_metres;
use crate::terrain::terrain_sample_grid_queries::{
    authored_terrain_sample_index, authored_water_height_for_sample,
};
use crate::terrain::terrain_world_sampling_types::AuthoredTerrainPoint;
use crate::terrain::terrain_world_sampling_types::TerrainBiomePoint;
use crate::terrain::terrain_world_sampling_types::TerrainPoint;
use crate::terrain::terrain_world_sampling_types::TerrainWaterPoint;

pub fn terrain_chunk_at(index: &TerrainIndex, world: Vec2) -> Option<Entity> {
    (index.chunk_span_m > 0.0 && world.is_finite())
        .then(|| {
            ((world - index.origin) / index.chunk_span_m)
                .floor()
                .as_ivec2()
        })
        .and_then(|coord| index.chunks.get(&coord).copied())
}

pub fn terrain_cell_indices(chunk: &TerrainChunk, world: Vec2) -> Option<UVec2> {
    Some(cell_at_grid_position(
        chunk.side,
        terrain_grid_position(chunk, world)?,
    ))
}

fn terrain_grid_position(chunk: &TerrainChunk, world: Vec2) -> Option<Vec2> {
    let max = f32::from(chunk.side.checked_sub(1)?);
    let grid = (world - chunk.origin) / chunk.spacing_m;
    (max > 0.0
        && chunk.spacing_m > 0.0
        && grid.is_finite()
        && grid.cmpge(Vec2::ZERO).all()
        && grid.cmple(Vec2::splat(max)).all())
    .then_some(grid)
}

fn cell_at_grid_position(side: u16, grid: Vec2) -> UVec2 {
    grid.floor()
        .as_uvec2()
        .min(UVec2::splat(u32::from(side) - 2))
}

#[allow(clippy::many_single_char_names)] // Compact corner notation makes the two triangle normals directly comparable.
pub fn terrain_cell_averaged_normal(
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
    let [first, second] = terrain_triangle_gradients([a, b, c, d], chunk.spacing_m, alternate);
    let normal = (first + second).normalize_or_zero();
    (normal != Vec3::ZERO).then_some(normal)
}

/// Upward triangle gradients in world-XZ corner order (00, 10, 01, 11).
#[allow(clippy::many_single_char_names)] // Corner notation follows the terrain sampler.
fn terrain_triangle_gradients([a, b, c, d]: [f32; 4], spacing: f32, alternate: bool) -> [Vec3; 2] {
    if alternate {
        [
            Vec3::new(a - b, spacing, a - c),
            Vec3::new(c - d, spacing, b - d),
        ]
    } else {
        [
            Vec3::new(c - d, spacing, a - c),
            Vec3::new(a - b, spacing, b - d),
        ]
    }
}

/// A cell route crosses triangle interiors, so both must meet the slope limit.
/// This conservative cell bound is not triangle connectivity or habitat membership.
pub fn minimum_terrain_triangle_normal_y(
    heights: [f32; 4],
    spacing: f32,
    alternate: bool,
) -> Option<f32> {
    if !spacing.is_finite() || spacing <= 0.0 || heights.iter().any(|height| !height.is_finite()) {
        return None;
    }
    let [first, second] =
        terrain_triangle_gradients(heights, spacing, alternate).map(Vec3::normalize_or_zero);
    (first != Vec3::ZERO && second != Vec3::ZERO).then_some(first.y.min(second.y))
}

pub fn sample_terrain(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<TerrainPoint> {
    Some(sample_terrain_surface(chunk, asset, edited, world)?.geometry)
}

pub struct TerrainSurfaceSample {
    pub geometry: TerrainPoint,
    triangle_vertices: [f32; 3],
}

impl TerrainSurfaceSample {
    pub fn depth_bounds(&self, water_height: f32) -> [f32; 2] {
        let [first, second, third] = self.triangle_vertices;
        [
            (water_height - first.max(second).max(third)).max(0.0),
            (water_height - first.min(second).min(third)).max(0.0),
        ]
    }
}

pub fn sample_terrain_surface(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<TerrainSurfaceSample> {
    let point = nearest_terrain_sample(chunk, asset, edited, world)?;
    sample_terrain_with_source(chunk, asset, edited, &point)
}

pub fn sample_terrain_biome(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<TerrainBiomePoint> {
    let point = nearest_terrain_sample(chunk, asset, edited, world)?;
    terrain_biome(asset, &point)
}

/// The terrain under a position and the nearest sample's visible water style.
pub fn sample_terrain_water(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<(TerrainPoint, Option<(u8, u16)>)> {
    let point = nearest_terrain_sample(chunk, asset, edited, world)?;
    let geometry = sample_terrain_with_source(chunk, asset, edited, &point)?.geometry;
    let (channel, style) = water_style(edited, &point);
    let biome = asset
        .canonical_terrain_grid()
        .biomes
        .get(usize::from(channel))?;
    let water = geometry
        .water_height_m
        .and(biome.water_presentation.as_ref())
        .map(|_| (channel, style));
    Some((geometry, water))
}

pub fn sample_authored_terrain(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<AuthoredTerrainPoint> {
    let point = nearest_terrain_sample(chunk, asset, edited, world)?;
    let geometry = sample_terrain_with_source(chunk, asset, edited, &point)?.geometry;
    let (biome_channel, style) = water_style(edited, &point);
    let water_biome = asset
        .canonical_terrain_grid()
        .biomes
        .get(usize::from(biome_channel))?;
    let water = geometry
        .water_height_m
        .and(water_biome.water_presentation.as_ref())
        .map(|presentation| TerrainWaterPoint {
            biome_channel,
            style,
            id: AssetId::from_key(&format!("{}/water/{style}", water_biome.name)),
            appearance: AssetId::from_key(&presentation.texture_animations[0].texture),
            flags: 0,
        });
    // The inherited biome remains the fallback for an invalid edited channel.
    Some(AuthoredTerrainPoint {
        geometry,
        biome: Some(terrain_biome(asset, &point)?),
        water,
    })
}

struct NearestTerrainSample<'a> {
    grid: Vec2,
    local_index: usize,
    source_index: usize,
    source: &'a openzt2_game_data::terrain::TerrainSample,
    edited_biome: Option<(u8, u8)>,
}

fn nearest_terrain_sample<'a>(
    chunk: &TerrainChunk,
    asset: &'a TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    world: Vec2,
) -> Option<NearestTerrainSample<'a>> {
    let grid = terrain_grid_position(chunk, world)?;
    let [x, z] = grid
        .round()
        .as_uvec2()
        .to_array()
        .map(|coordinate| coordinate as usize);
    let local_index = calculate_row_major_terrain_sample_index(chunk.side, x, z)?;
    let source_index = authored_terrain_sample_index(chunk, asset, x, z)?;
    let source = asset.canonical_terrain_grid().samples.get(source_index)?;
    let edited_biome = edited.and_then(|edits| {
        edits
            .blend_weights
            .get(local_index * 16..(local_index + 1) * 16)?
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|&(channel, weight)| (weight, std::cmp::Reverse(channel)))
            .map(|(channel, weight)| (channel as u8, weight))
    });
    Some(NearestTerrainSample {
        grid,
        local_index,
        source_index,
        source,
        edited_biome,
    })
}

fn terrain_biome(
    asset: &TerrainAsset,
    point: &NearestTerrainSample<'_>,
) -> Option<TerrainBiomePoint> {
    let biomes = &asset.canonical_terrain_grid().biomes;
    let inherited = biomes.get(usize::from(point.source.biome_index))?;
    let (channel, weight, biome) = point
        .edited_biome
        .and_then(|(channel, weight)| Some((channel, weight, biomes.get(usize::from(channel))?)))
        .unwrap_or((point.source.biome_index, u8::MAX, inherited));
    Some(TerrainBiomePoint {
        id: AssetId::from_key(&biome.name),
        channel,
        weight,
    })
}

fn water_style(
    edited: Option<&EditedTerrainSamples>,
    point: &NearestTerrainSample<'_>,
) -> (u8, u16) {
    let channel = edited
        .and_then(|edits| edits.water_biome_channels.get(point.local_index).copied())
        .unwrap_or(point.source.biome_index);
    let style = edited
        .and_then(|edits| edits.water_styles.get(point.local_index).copied())
        .unwrap_or(match point.source.water_depth {
            TerrainWaterDepth::Dry => 0,
            TerrainWaterDepth::Shallow => 1,
            TerrainWaterDepth::Deep => 2,
        });
    (channel, style)
}

fn sample_terrain_with_source(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    point: &NearestTerrainSample<'_>,
) -> Option<TerrainSurfaceSample> {
    let cell = cell_at_grid_position(chunk.side, point.grid);
    let local = point.grid - cell.as_vec2();
    let x = cell.x as usize;
    let z = cell.y as usize;
    let sample_height = |x, z| terrain_sample_height_metres(chunk, asset, edited, x, z);
    let [a, b, c, d] = [
        sample_height(x, z)?,
        sample_height(x + 1, z)?,
        sample_height(x, z + 1)?,
        sample_height(x + 1, z + 1)?,
    ];
    let alternate = terrain_cell_uses_alternate_triangle_diagonal(chunk, asset, cell)?;
    let [first, second] = terrain_triangle_gradients([a, b, c, d], chunk.spacing_m, alternate);
    let (height_m, gradient, triangle_vertices) = if alternate {
        if local.x + local.y <= 1.0 {
            (a + (b - a) * local.x + (c - a) * local.y, first, [a, b, c])
        } else {
            (
                d + (c - d) * (1.0 - local.x) + (b - d) * (1.0 - local.y),
                second,
                [d, c, b],
            )
        }
    } else if local.x <= local.y {
        (a + (c - a) * local.y + (d - c) * local.x, first, [a, c, d])
    } else {
        (a + (b - a) * local.x + (d - b) * local.y, second, [a, b, d])
    };
    let water_height_m = if let Some(edited) = edited {
        (*edited.water_styles.get(point.local_index)? != 0)
            .then_some(f32::from(*edited.water_cm.get(point.local_index)?) * 0.01)
            .filter(|water| *water > height_m)
    } else {
        authored_water_height_for_sample(asset, point.source_index, point.source, height_m)
    };
    let surface = point
        .edited_biome
        .map_or(point.source.biome_index, |(channel, _)| channel);
    Some(TerrainSurfaceSample {
        geometry: TerrainPoint {
            height_m,
            water_height_m,
            surface: u32::from(surface),
            normal: gradient.normalize_or_zero(),
        },
        triangle_vertices,
    })
}
