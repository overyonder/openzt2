use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::topology::fence_segment_presentation_types::{FenceSegmentPrefab, FenceSegmentShape, FenceTerrainEndpointPresentationCells};
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid};

pub(super) type SelectorFences<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static FenceEdge,
        Option<&'static FenceSegmentPrefab>,
        Option<&'static FenceTerrainEndpointPresentationCells>,
    ),
>;

type EdgeKey = ((i32, i32), (i32, i32));

fn edge_key(first: IVec2, second: IVec2) -> EdgeKey {
    let first = (first.x, first.y);
    let second = (second.x, second.y);
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}

fn selector_shape(prefab: &FenceSegmentPrefab, first: Vec2, second: Vec2) -> u8 {
    let flip = u8::from(prefab.mirror_source_y) * 4;
    let quadrant = match (second.x > first.x, second.y > first.y) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 3,
        (false, false) => 2,
    };
    match prefab.shape {
        FenceSegmentShape::Straight => 0,
        FenceSegmentShape::CardinalCurve90 => {
            let direction = if second.x > first.x {
                1
            } else if second.x < first.x {
                2
            } else if second.y > first.y {
                3
            } else if second.y < first.y {
                4
            } else {
                return 0;
            };
            direction + flip
        }
        FenceSegmentShape::DiagonalCurve90 => 9 + quadrant + flip,
        FenceSegmentShape::CardinalCurve135 => {
            let direction = if second.x > first.x {
                2
            } else if second.x < first.x {
                4
            } else if second.y > first.y {
                1
            } else {
                3
            };
            16 + direction + flip
        }
        FenceSegmentShape::DiagonalCurve135 => 25 + quadrant + (4 - flip),
    }
}

/// Writes occur in cardinal order followed by the two diagonal assignments.
fn apply_shape(codes: &mut [u8; 4], direction: usize, shape: u8) {
    let writes: &[(usize, u8)] = match (direction, shape) {
        (0, 1) => &[(0, 1)],
        (0, 2) => &[(0, 3)],
        (0, 5) => &[(0, 4)],
        (0, 6) => &[(0, 2)],
        (0, 18) => &[(0, 18), (1, 12)],
        (0, 20) => &[(0, 15)],
        (0, 22) => &[(0, 22)],
        (0, 24) => &[(0, 11), (3, 19)],
        (1, 3) => &[(1, 3)],
        (1, 4) => &[(1, 1)],
        (1, 7) => &[(1, 2)],
        (1, 8) => &[(1, 4)],
        (1, 17) => &[(1, 15)],
        (1, 19) => &[(1, 18), (2, 12)],
        (1, 21) => &[(1, 11), (0, 19)],
        (1, 23) => &[(1, 22)],
        (2, 1) => &[(2, 3)],
        (2, 2) => &[(2, 1)],
        (2, 5) => &[(2, 2)],
        (2, 6) => &[(2, 4)],
        (2, 18) => &[(2, 15)],
        (2, 20) => &[(2, 18), (3, 12)],
        (2, 22) => &[(2, 11), (1, 19)],
        (2, 24) => &[(2, 22)],
        (3, 3) => &[(3, 1)],
        (3, 4) => &[(3, 3)],
        (3, 7) => &[(3, 4)],
        (3, 8) => &[(3, 2)],
        (3, 17) => &[(3, 18), (0, 12)],
        (3, 19) => &[(3, 15)],
        (3, 21) => &[(3, 22)],
        (3, 23) => &[(3, 11), (2, 19)],
        (4, 9) => &[(0, 10), (1, 5), (2, 7)],
        (4, 11) => &[(2, 10), (3, 5), (0, 7)],
        (4, 13) => &[(0, 6), (1, 9), (3, 8)],
        (4, 15) => &[(2, 6), (3, 9), (1, 8)],
        (4, 25) => &[(0, 23), (3, 24), (1, 13), (2, 14)],
        (4, 27) => &[(2, 23), (1, 24), (3, 13), (0, 14)],
        (4, 29) => &[(0, 20), (1, 16), (2, 17), (3, 21)],
        (4, 31) => &[(2, 20), (3, 16), (0, 17), (1, 21)],
        (5, 12) => &[(0, 5), (3, 10), (1, 7)],
        (5, 10) => &[(2, 5), (1, 10), (3, 7)],
        (5, 16) => &[(0, 9), (3, 6), (2, 8)],
        (5, 14) => &[(2, 9), (1, 6), (0, 8)],
        (5, 26) => &[(2, 13), (1, 23), (0, 24), (3, 14)],
        (5, 28) => &[(0, 13), (3, 23), (2, 24), (1, 14)],
        (5, 30) => &[(2, 16), (3, 17), (1, 20), (0, 21)],
        (5, 32) => &[(0, 16), (1, 17), (3, 20), (2, 21)],
        _ => &[],
    };
    for &(side, code) in writes {
        codes[side] = code;
    }
}

type FenceProbeSegments = Box<[[Vec2; 2]]>;
pub(super) type SelectorTilePatch = (UVec2, [u8; 4], [FenceProbeSegments; 8]);
type FenceSelection = (u8, Vec<[Vec2; 2]>);

fn source_assignments(
    edge: &FenceEdge,
    prefab: Option<&FenceSegmentPrefab>,
    endpoints: Option<&FenceTerrainEndpointPresentationCells>,
    spacing: f32,
    definitions: WorldDefinitionsView<'_>,
    topology: &TopologyGrid,
) -> Option<Vec<(EdgeKey, u8, Vec<[Vec2; 2]>)>> {
    let curb = openzt2_game_data::AssetId::from_key("curb");
    let mut assignments = Vec::new();
    let object = definitions
        .find_fence(edge.definition)
        .and_then(|definition| definitions.find_object(definition.object))
        .or_else(|| definitions.find_object(edge.definition))?;
    if object.behavior_type_identifiers.contains(&curb) {
        return Some(Vec::new());
    }
    let prefab = prefab?;
    let [first, second] = endpoints?.endpoint_cells().map(|cell| {
        let world = topology.cell_translation(cell);
        Vec2::new(world.x, -world.z) / spacing
    });
    if !first.is_finite() || !second.is_finite() {
        return None;
    }
    let shape = selector_shape(prefab, first, second);
    let fence = definitions.find_fence(edge.definition)?;
    let binder = match (prefab.shape, first.x != second.x && first.y != second.y) {
        (FenceSegmentShape::Straight, false) => "fence90",
        (FenceSegmentShape::Straight, true) => "fence45",
        (FenceSegmentShape::CardinalCurve90, _) => "fence90curve90",
        (FenceSegmentShape::DiagonalCurve90, _) => "fence45curve90",
        (FenceSegmentShape::CardinalCurve135, _) => "fence90curve135",
        (FenceSegmentShape::DiagonalCurve135, _) => "fence45curve135",
    };
    let start = first * spacing;
    let end = second * spacing;
    let axis = (end - start).normalize_or_zero();
    let normal = Vec2::new(-axis.y, axis.x) * if prefab.mirror_source_y { -1.0 } else { 1.0 };
    let curve = fence
        .fitting_curves
        .iter()
        .find(|curve| curve.binder == openzt2_game_data::AssetId::from_key(binder));
    let segments: Vec<[Vec2; 2]> =
        if let Some(curve) = curve.filter(|curve| !curve.points.is_empty()) {
            let mut previous = start;
            curve
                .points
                .iter()
                .map(|point| {
                    let next = start + axis * point[0] + normal * point[1];
                    let segment = [previous, next];
                    previous = next;
                    segment
                })
                .collect()
        } else {
            vec![[start - axis * 0.1, end + axis * 0.1]]
        };
    let delta = second - first;
    if delta == Vec2::ZERO
        || (delta.x != 0.0 && delta.y != 0.0 && (delta.x.abs() - delta.y.abs()).abs() > 0.001)
    {
        return None;
    }
    // Placement keys identify cells rather than requiring world endpoints
    // to coincide with a terrain-grid vertex. Keep the fitting curve in
    // its actual world position; only its assignment keys are quantized.
    let first_cell = first.floor().as_ivec2();
    let second_cell = second.floor().as_ivec2();
    let direction = IVec2::new(
        i32::from(delta.x > 0.0) - i32::from(delta.x < 0.0),
        i32::from(delta.y > 0.0) - i32::from(delta.y < 0.0),
    );
    let steps = (second_cell - first_cell).abs().max_element().max(1);
    for step in 0..steps {
        let key = edge_key(
            first_cell + direction * step,
            first_cell + direction * (step + 1),
        );
        assignments.push((key, shape, segments.clone()));
    }
    Some(assignments)
}

#[derive(Component, Default)]
pub(in crate::terrain) struct TerrainWaterSelectorProjection {
    by_source: bevy::ecs::entity::EntityHashMap<Box<[EdgeKey]>>,
    edges: BTreeMap<EdgeKey, BTreeMap<Entity, FenceSelection>>,
}

impl TerrainWaterSelectorProjection {
    pub(super) fn empty(&self) -> bool {
        self.edges.is_empty()
    }

    pub(super) fn patch(
        &mut self,
        changed: impl IntoIterator<Item = Entity>,
        asset: &TerrainAsset,
        definitions: WorldDefinitionsView<'_>,
        topology: &TopologyGrid,
        fences: &SelectorFences,
    ) -> Option<Vec<SelectorTilePatch>> {
        let grid = asset.canonical_terrain_grid();
        let spacing = grid.units.cell_size_metres * 4.0;
        if !spacing.is_finite() || spacing <= 0.0 {
            return None;
        }
        let dimensions = UVec2::new(
            grid.width.checked_sub(1)? / 4,
            grid.height.checked_sub(1)? / 4,
        );
        let contributions = changed
            .into_iter()
            .map(|entity| {
                let next = if let Ok((_, edge, prefab, endpoints)) = fences.get(entity) {
                    source_assignments(edge, prefab, endpoints, spacing, definitions, topology)?
                } else {
                    Vec::new()
                };
                Some((entity, next))
            })
            .collect::<Option<Vec<_>>>()?;
        let mut dirty_edges = std::collections::BTreeSet::new();
        for (entity, next) in contributions {
            if let Some(previous) = self.by_source.remove(&entity) {
                for edge in previous.iter().copied() {
                    if let std::collections::btree_map::Entry::Occupied(mut entry) =
                        self.edges.entry(edge)
                    {
                        entry.get_mut().remove(&entity);
                        if entry.get().is_empty() {
                            entry.remove();
                        }
                    }
                    dirty_edges.insert(edge);
                }
            }
            if !next.is_empty() {
                self.by_source
                    .insert(entity, next.iter().map(|(edge, ..)| *edge).collect());
            }
            for (edge, shape, segments) in next {
                self.edges
                    .entry(edge)
                    .or_default()
                    .insert(entity, (shape, segments));
                dirty_edges.insert(edge);
            }
        }
        let mut tiles = bevy::platform::collections::HashSet::default();
        for ((ax, az), (bx, bz)) in dirty_edges {
            let minimum = IVec2::new(ax.min(bx), az.min(bz));
            for y in -1..=0 {
                for x in -1..=0 {
                    let tile = minimum + IVec2::new(x, y);
                    if tile.min_element() >= 0 && tile.as_uvec2().cmplt(dimensions).all() {
                        tiles.insert(tile.as_uvec2());
                    }
                }
            }
        }
        Some(
            tiles
                .into_iter()
                .map(|tile| self.project_tile(tile))
                .collect(),
        )
    }

    pub(super) fn project_tile(&self, tile: UVec2) -> SelectorTilePatch {
        let origin = tile.as_ivec2();
        let corners = [
            origin,
            origin + IVec2::X,
            origin + IVec2::ONE,
            origin + IVec2::Y,
        ];
        let edges = [(3, 2), (1, 2), (0, 1), (0, 3), (0, 2), (1, 3)];
        let directional_edges = [
            (3, 2),
            (0, 2),
            (1, 2),
            (1, 3),
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 3),
        ];
        let assignments =
            |(first, second)| self.edges.get(&edge_key(corners[first], corners[second]));
        let probes = std::array::from_fn(|direction| {
            assignments(directional_edges[direction]).map_or_else(Box::default, |edge| {
                edge.values()
                    .flat_map(|(_, segments)| segments.iter().copied())
                    .collect()
            })
        });
        let mut codes = [0; 4];
        for (direction, edge) in edges.into_iter().enumerate() {
            if let Some((_, (shape, _))) = assignments(edge).and_then(|edge| edge.first_key_value())
            {
                apply_shape(&mut codes, direction, *shape);
            }
        }
        (tile, codes, probes)
    }
}
