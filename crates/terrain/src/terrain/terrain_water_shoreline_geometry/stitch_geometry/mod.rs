use bevy::prelude::{Entity, UVec2};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionView,
};
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainShorelinePatch;

mod selector_polygons;
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainWaterStitchGeometry;

/// The geometric-wave particle path uses the deformed five-by-five grid and
/// retains its third-coordinate horizontal displacement weights.
pub(crate) fn construct_terrain_water_geometric_wake_tile(
    terrain: &TerrainAsset,
    membership: &TerrainWaterMembershipIndex,
    owner: Entity,
    source: &TerrainWaterRegionView<'_>,
    tile: UVec2,
) -> Option<TerrainWaterStitchGeometry> {
    if membership.terrain() != source.terrain().id() {
        return None;
    }
    let grid = terrain.canonical_terrain_grid();
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
            let sample = grid
                .samples
                .get((point.y * grid.width + point.x) as usize)?;
            patch.positions[(y * 5 + x) as usize] = [
                point.x as f32 * grid.units.cell_size_metres,
                point.y as f32 * grid.units.cell_size_metres,
                if sample.water_surface_flag { 0.0 } else { 1.0 },
            ];
        }
    }
    let mut indices = Vec::with_capacity(96);
    super::mesh_construction::append_deformed_patch_indices(&mut patch, 0, &mut indices);
    Some(TerrainWaterStitchGeometry {
        positions: patch.positions.into(),
        indices,
    })
}

/// Returns vertex and triangle counts. Unknown selector topology is unresolved.
pub(crate) fn count_terrain_water_stitch_tile(
    membership: &TerrainWaterMembershipIndex,
    owner: Entity,
    tile: UVec2,
) -> Option<(usize, usize)> {
    let sides = membership.side_owners(tile)?;
    let codes = membership.selector_codes(tile)?;
    if codes.iter().all(|code| *code == 0) && sides.iter().all(|side| *side == Some(owner)) {
        return Some((4, 2));
    }
    let mut count = (0, 0);
    for (side, code) in sides.iter().zip(codes) {
        if *side != Some(owner) {
            continue;
        }
        let added = match code {
            0 | 7 | 8 | 17 | 24 => (3, 1),
            1 | 2 | 5 | 6 | 11 | 18 => (4, 2),
            3 | 4 | 9 | 10 | 15 | 22 => (7, 3),
            13 | 16 | 20 | 23 => (5, 3),
            12 | 14 | 19 | 21 => (0, 0),
            _ => return None,
        };
        count.0 += added.0;
        count.1 += added.1;
    }
    Some(count)
}

/// Builds the separately tessellated terrain-clipped particle patch. Positions
/// use source XYZ with the region plane as Z; consumers supply their own UVs.
pub(crate) fn construct_terrain_water_stitch_tile(
    terrain: &TerrainAsset,
    membership: &TerrainWaterMembershipIndex,
    owner: Entity,
    source: &TerrainWaterRegionView<'_>,
    tile: UVec2,
) -> Option<TerrainWaterStitchGeometry> {
    if membership.terrain() != source.terrain().id() {
        return None;
    }
    let sides = membership.side_owners(tile)?;
    let codes = membership.selector_codes(tile)?;
    let plane = source.surface_height(terrain)?;
    let spacing = terrain.canonical_terrain_grid().units.cell_size_metres;
    let origin = tile * 4;
    let point = |point: UVec2| [point.x as f32 * spacing, point.y as f32 * spacing, plane];
    let corners = [
        origin,
        origin + UVec2::X * 4,
        origin + UVec2::Y * 4,
        origin + UVec2::splat(4),
    ]
    .map(point);
    if codes.iter().all(|code| *code == 0) && sides.iter().all(|side| *side == Some(owner)) {
        return Some(TerrainWaterStitchGeometry {
            positions: vec![corners[0], corners[1], corners[3], corners[2]],
            indices: vec![0, 1, 3, 3, 1, 2],
        });
    }
    let center = point(origin + UVec2::splat(2));
    let (vertex_count, triangle_count) = count_terrain_water_stitch_tile(membership, owner, tile)?;
    let mut geometry = TerrainWaterStitchGeometry {
        positions: Vec::with_capacity(vertex_count),
        indices: Vec::with_capacity(triangle_count * 3),
    };
    for (side, candidate) in sides.iter().enumerate() {
        if *candidate == Some(owner) {
            selector_polygons::append_selector_stitch_side(
                &mut geometry,
                center,
                spacing * 2.0,
                side,
                codes[side],
            )?;
        }
    }
    Some(geometry)
}
