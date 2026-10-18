use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::{Entity, IVec2, UVec2, Vec2};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainSample;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionView,
};
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::{
    TerrainShorelineBoundaryEdge, TerrainShorelinePatch, TerrainShorelineProfile,
    TerrainShorelineProfilePoint,
};

use super::boundary_tracing::trace_shoreline_boundary;
use super::{construct_terrain_shoreline_profile, sample_terrain_shoreline_profile};

const NEIGHBOURS: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

struct ShorelineTerrain<'a> {
    asset: &'a TerrainAsset,
    membership: &'a TerrainWaterMembershipIndex,
    owner: Entity,
    plane: f32,
}

impl ShorelineTerrain<'_> {
    fn sample(&self, point: IVec2) -> Option<&TerrainSample> {
        let grid = self.asset.canonical_terrain_grid();
        if point.min_element() < 0 || point.x as u32 >= grid.width || point.y as u32 >= grid.height
        {
            return None;
        }
        grid.samples
            .get((point.y as u32 * grid.width + point.x as u32) as usize)
    }

    fn height(&self, point: IVec2) -> Option<f32> {
        self.membership
            .source_point_height(point.as_uvec2(), self.asset)
    }

    fn candidate(&self, edge: TerrainShorelineBoundaryEdge) -> bool {
        let (Some(land), Some(water)) = (self.sample(edge.land), self.sample(edge.water)) else {
            return false;
        };
        self.membership.point_owner(edge.water.as_uvec2()) == Some(self.owner)
            && !water.water_surface_flag
            && self
                .height(edge.water)
                .is_some_and(|height| height < self.plane)
            && (land.water_surface_flag
                || (self.membership.point_owner(edge.land.as_uvec2()) == Some(self.owner)
                    && self
                        .height(edge.land)
                        .is_some_and(|height| height >= self.plane)))
    }

    fn seed(&self, land: IVec2) -> Option<TerrainShorelineBoundaryEdge> {
        NEIGHBOURS
            .into_iter()
            .map(|offset| TerrainShorelineBoundaryEdge {
                land,
                water: land + offset,
            })
            .find(|edge| self.candidate(*edge))
    }

    fn intersection(&self, edge: TerrainShorelineBoundaryEdge) -> Option<(Vec2, Vec2)> {
        let spacing = self.asset.canonical_terrain_grid().units.cell_size_metres;
        let land = edge.land.as_vec2() * spacing;
        let water = edge.water.as_vec2() * spacing;
        let position = if self.sample(edge.land)?.water_surface_flag {
            land
        } else {
            let height = self.height(edge.land)?;
            land + (water - land) * ((self.plane - height) / (self.height(edge.water)? - height))
        };
        Some((position, position + (water - land).normalize_or_zero()))
    }
}

pub(super) fn construct_source_region_shoreline_patches(
    asset: &TerrainAsset,
    membership: &TerrainWaterMembershipIndex,
    owner: Entity,
    source: &TerrainWaterRegionView<'_>,
) -> Option<Vec<(UVec2, TerrainShorelinePatch)>> {
    if membership.terrain() != source.terrain().id() {
        return None;
    }
    for tile in membership.render_tiles(owner) {
        membership.selector_codes(tile)?;
    }
    let terrain = ShorelineTerrain {
        asset,
        membership,
        owner,
        plane: source.surface_height(asset)?,
    };
    let mut seeds = BTreeSet::new();
    for point in membership.accepted_points(owner) {
        let point = point.as_ivec2();
        if terrain.height(point)? >= terrain.plane && terrain.seed(point).is_some() {
            seeds.insert((point.y, point.x));
        }
        for offset in NEIGHBOURS {
            let land = point + offset;
            if terrain
                .sample(land)
                .is_some_and(|sample| sample.water_surface_flag)
                && terrain.seed(land).is_some()
            {
                seeds.insert((land.y, land.x));
            }
        }
    }
    let mut patches = Vec::new();
    while let Some((y, x)) = seeds.pop_first() {
        let Some(seed) = terrain.seed(IVec2::new(x, y)) else {
            continue;
        };
        let boundary = trace_shoreline_boundary(seed, |edge| terrain.candidate(edge)).ok()?;
        for edge in &boundary.edges {
            seeds.remove(&(edge.land.y, edge.land.x));
        }
        let crossings = boundary
            .edges
            .iter()
            .map(|edge| terrain.intersection(*edge))
            .collect::<Option<Vec<_>>>()?;
        let mut points = Vec::with_capacity(crossings.len());
        let mut longitudinal = 0.0;
        for (index, (position, _)) in crossings.iter().enumerate() {
            if index > 0 {
                longitudinal += 0.1 * crossings[index - 1].0.distance(*position);
            }
            points.push(TerrainShorelineProfilePoint {
                position: (*position * 256.0).floor() / 256.0,
                longitudinal_coordinate: longitudinal,
            });
        }
        let grid = asset.canonical_terrain_grid();
        let tile_span = grid.units.cell_size_metres * 4.0;
        let tile_count = UVec2::new((grid.width - 1) / 4, (grid.height - 1) / 4);
        let mut tile_profiles = BTreeMap::<(u32, u32), Vec<usize>>::new();
        let segments = if boundary.closed {
            crossings.len()
        } else {
            crossings.len().saturating_sub(1)
        };
        for index in 0..segments {
            let next = (index + 1) % crossings.len();
            let quad = [
                crossings[index].0,
                crossings[next].0,
                crossings[next].1,
                crossings[index].1,
            ];
            let minimum = quad.into_iter().fold(Vec2::splat(f32::INFINITY), Vec2::min);
            let maximum = quad
                .into_iter()
                .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
            let low = (minimum / tile_span).floor().max(Vec2::ZERO).as_uvec2();
            let high = (maximum / tile_span)
                .floor()
                .max(Vec2::ZERO)
                .as_uvec2()
                .min(UVec2::new(
                    tile_count.x.saturating_sub(1),
                    tile_count.y.saturating_sub(1),
                ));
            for y in low.y..=high.y {
                for x in low.x..=high.x {
                    let tile = UVec2::new(x, y);
                    if quad_intersects_tile(quad, tile.as_vec2() * tile_span, tile_span) {
                        tile_profiles.entry((y, x)).or_default().push(index);
                    }
                }
            }
        }
        let used = tile_profiles
            .values()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        if used.is_empty() {
            continue;
        }
        let profile = construct_terrain_shoreline_profile(
            used.iter().map(|index| points[*index]).collect(),
            1.0,
        )
        .ok()?;
        for (index, normalized) in used.into_iter().zip(profile.points()) {
            points[index] = *normalized;
        }
        let mut accumulated = BTreeMap::<u32, (Vec2, u32)>::new();
        let mut included = Vec::new();
        for (&(y, x), indices) in &tile_profiles {
            let tile = UVec2::new(x, y);
            if !membership
                .side_owners(tile)?
                .iter()
                .any(|side| *side == Some(owner))
            {
                continue;
            }
            let profile = TerrainShorelineProfile::from_normalized_points(
                indices.iter().map(|index| points[*index]).collect(),
                1.0,
            )
            .ok()?;
            for y in 0..5 {
                for x in 0..5 {
                    let point = tile * 4 + UVec2::new(x, y);
                    let uv = sample_terrain_shoreline_profile(
                        &profile,
                        point.as_vec2() * grid.units.cell_size_metres,
                        membership.source_point_height(point, asset)?,
                        terrain.plane,
                    );
                    let entry = accumulated
                        .entry(point.y * grid.width + point.x)
                        .or_default();
                    entry.0 += uv;
                    entry.1 += 1;
                }
            }
            included.push(tile);
        }
        for tile in included {
            let mut patch = TerrainShorelinePatch {
                positions: [[0.0; 3]; 25],
                texture_coordinates: [[0.0; 2]; 25],
                included_sides: membership
                    .side_owners(tile)?
                    .map(|side| side == Some(owner)),
                selector_codes: *membership.selector_codes(tile)?,
            };
            for y in 0..5 {
                for x in 0..5 {
                    let point = tile * 4 + UVec2::new(x, y);
                    let index = point.y * grid.width + point.x;
                    let sample = grid.samples.get(index as usize)?;
                    let vertex = (y * 5 + x) as usize;
                    patch.positions[vertex] = [
                        point.x as f32 * grid.units.cell_size_metres,
                        point.y as f32 * grid.units.cell_size_metres,
                        if sample.water_surface_flag { 0.0 } else { 1.0 },
                    ];
                    if let Some((uv, count)) = accumulated.get(&index) {
                        patch.texture_coordinates[vertex] = (*uv / *count as f32).to_array();
                    }
                }
            }
            patches.push((tile, patch));
        }
    }
    Some(patches)
}

fn quad_intersects_tile(quad: [Vec2; 4], origin: Vec2, span: f32) -> bool {
    let corners = [
        origin,
        origin + Vec2::X * span,
        origin + Vec2::splat(span),
        origin + Vec2::Y * span,
    ];
    for (start, end) in quad.into_iter().zip(quad.into_iter().cycle().skip(1)) {
        let direction = end - start;
        if direction.length_squared() < 0.0001 {
            continue;
        }
        if corners.into_iter().all(|corner| {
            let relative = corner - start;
            relative.x * direction.y - relative.y * direction.x >= 0.0
        }) {
            return false;
        }
    }
    true
}
