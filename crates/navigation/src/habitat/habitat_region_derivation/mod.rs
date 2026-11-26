use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitatFenceSegment {
    pub fence_entity: Entity,
    pub first_topology_node: IVec2,
    pub second_topology_node: IVec2,
    pub boundary_is_breached: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DerivedHabitatRegion {
    pub topology_cells: Vec<IVec2>,
    pub(super) sectors: Vec<(IVec2, u8)>,
    pub boundary_fence_entities: Vec<Entity>,
    pub boundary_is_breached: bool,
}

use super::placement_sectors::{cardinal_step, PlacementSector};

type Face = (IVec2, u8);
type BlockedBoundary = HashMap<(Face, Face), (Entity, bool)>;

fn boundary_key(a: Face, b: Face) -> (Face, Face) {
    let order = |(tile, side): Face| (tile.x, tile.y, side);
    if order(a) <= order(b) {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn derive_enclosed_habitat_regions_from_fence_segments(
    fence_segments: &[HabitatFenceSegment],
    changed_topology_bounds: IRect,
) -> Vec<DerivedHabitatRegion> {
    let expanded = split_fence_segments_at_topology_nodes(fence_segments);
    let relevant: Vec<_> = connected_fence_components(&expanded)
        .into_iter()
        .filter(|component| component.touches(changed_topology_bounds))
        .flat_map(|component| component.segments)
        .collect();
    derive_enclosed_habitats(&relevant)
}

fn record_fence_boundary(blocked: &mut BlockedBoundary, segment: HabitatFenceSegment) {
    let a = segment.first_topology_node;
    let b = segment.second_topology_node;
    let tile = a.min(b);
    let delta = b - a;
    let mut block = |a, b| {
        blocked.insert(
            boundary_key(a, b),
            (segment.fence_entity, segment.boundary_is_breached),
        );
    };
    match (delta.x, delta.y) {
        (1 | -1, 0) => {
            let record = PlacementSector {
                tile: IVec2::new(tile.x, a.y),
                layer: 0,
                side: 0,
            };
            if let [Some(first), Some(second)] = record.expanded(|_, _| true) {
                block((first.tile, first.side), (second.tile, second.side));
            }
        }
        (0, 1 | -1) => {
            let record = PlacementSector {
                tile: IVec2::new(a.x, tile.y),
                layer: 0,
                side: 6,
            };
            if let [Some(first), Some(second)] = record.expanded(|_, _| true) {
                block((first.tile, first.side), (second.tile, second.side));
            }
        }
        (1 | -1, 1 | -1) if delta.x == delta.y => {
            block((tile, 0), (tile, 6));
            block((tile, 4), (tile, 2));
        }
        (1 | -1, 1 | -1) => {
            block((tile, 0), (tile, 2));
            block((tile, 4), (tile, 6));
        }
        _ => {}
    }
}

fn neighbouring_sectors((tile, side): Face) -> [Face; 3] {
    [
        (tile, (side + 6) % 8),
        (tile, (side + 2) % 8),
        (tile + cardinal_step(side), (side + 4) % 8),
    ]
}

fn flood_sectors(
    queue: &mut Vec<Face>,
    seen: &mut HashSet<Face>,
    min: IVec2,
    max: IVec2,
    blocked: &BlockedBoundary,
) {
    let mut cursor = 0;
    while let Some(&face) = queue.get(cursor) {
        cursor += 1;
        for neighbour in neighbouring_sectors(face) {
            if topology_cell_is_inside_exclusive_maximum_bounds(neighbour.0, min, max)
                && !blocked.contains_key(&boundary_key(face, neighbour))
                && seen.insert(neighbour)
            {
                queue.push(neighbour);
            }
        }
    }
}

pub(super) fn derive_enclosed_habitats(
    relevant: &[HabitatFenceSegment],
) -> Vec<DerivedHabitatRegion> {
    let Some(first) = relevant.first() else {
        return Vec::new();
    };
    let mut min = first.first_topology_node;
    let mut max = min;
    let mut blocked = HashMap::with_capacity(relevant.len() * 2);
    for &segment in relevant {
        min = min
            .min(segment.first_topology_node)
            .min(segment.second_topology_node);
        max = max
            .max(segment.first_topology_node)
            .max(segment.second_topology_node);
        record_fence_boundary(&mut blocked, segment);
    }
    min -= IVec2::ONE;
    max += IVec2::ONE;
    let mut seen = HashSet::new();
    let seed = (min, 0);
    seen.insert(seed);
    flood_sectors(&mut vec![seed], &mut seen, min, max, &blocked);

    let mut regions = Vec::new();
    for x in min.x..max.x {
        for y in min.y..max.y {
            for side in [0, 2, 4, 6] {
                let first = (IVec2::new(x, y), side);
                if !seen.insert(first) {
                    continue;
                }
                let mut faces = vec![first];
                flood_sectors(&mut faces, &mut seen, min, max, &blocked);
                faces.sort_unstable_by_key(|(tile, side)| (tile.x, tile.y, *side));
                let mut boundary = Vec::new();
                let mut breached = false;
                for &face in &faces {
                    for neighbour in neighbouring_sectors(face) {
                        if let Some(&(fence, breach)) = blocked.get(&boundary_key(face, neighbour))
                        {
                            boundary.push(fence);
                            breached |= breach;
                        }
                    }
                }
                boundary.sort_unstable();
                boundary.dedup();
                let mut topology_cells: Vec<_> = faces.iter().map(|(tile, _)| *tile).collect();
                topology_cells.dedup();
                regions.push(DerivedHabitatRegion {
                    topology_cells,
                    sectors: faces,
                    boundary_fence_entities: boundary,
                    boundary_is_breached: breached,
                });
            }
        }
    }
    regions
}

pub(super) fn split_fence_segments_at_topology_nodes(
    segments: &[HabitatFenceSegment],
) -> Vec<HabitatFenceSegment> {
    let mut result = Vec::new();
    for segment in segments {
        let a = segment.first_topology_node;
        let b = segment.second_topology_node;
        let delta = b - a;
        let length = delta.x.unsigned_abs().max(delta.y.unsigned_abs());
        if length == 0
            || (delta.x != 0 && delta.y != 0 && delta.x.unsigned_abs() != delta.y.unsigned_abs())
        {
            continue;
        }
        let step = delta.signum();
        for offset in 0..length {
            let first = a + step * offset as i32;
            result.push(HabitatFenceSegment {
                first_topology_node: first,
                second_topology_node: first + step,
                ..*segment
            });
        }
    }
    result
}

pub(super) struct FenceComponent {
    pub(super) segments: Vec<HabitatFenceSegment>,
    pub(super) bounds: IRect,
}

impl FenceComponent {
    pub(super) fn touches(&self, bounds: IRect) -> bool {
        self.segments.iter().any(|edge| {
            inclusive_topology_bounds_contain_point(bounds, edge.first_topology_node)
                || inclusive_topology_bounds_contain_point(bounds, edge.second_topology_node)
        })
    }
}

pub(super) fn connected_fence_components(segments: &[HabitatFenceSegment]) -> Vec<FenceComponent> {
    let mut by_node: HashMap<IVec2, Vec<usize>> = HashMap::new();
    for (index, segment) in segments.iter().enumerate() {
        by_node
            .entry(segment.first_topology_node)
            .or_default()
            .push(index);
        by_node
            .entry(segment.second_topology_node)
            .or_default()
            .push(index);
    }
    let mut visited = vec![false; segments.len()];
    let mut result = Vec::new();
    for first in 0..segments.len() {
        if visited[first] {
            continue;
        }
        visited[first] = true;
        let mut component = vec![first];
        let mut cursor = 0;
        let mut bounds = IRect::from_corners(
            segments[first].first_topology_node,
            segments[first].second_topology_node,
        );
        while let Some(&edge_index) = component.get(cursor) {
            cursor += 1;
            let edge = segments[edge_index];
            bounds = bounds.union(IRect::from_corners(
                edge.first_topology_node,
                edge.second_topology_node,
            ));
            for topology_node in [edge.first_topology_node, edge.second_topology_node] {
                if let Some(connected_fence_segment_indexes) = by_node.get(&topology_node) {
                    for &neighbor in connected_fence_segment_indexes {
                        if !visited[neighbor] {
                            visited[neighbor] = true;
                            component.push(neighbor);
                        }
                    }
                }
            }
        }
        result.push(FenceComponent {
            segments: component.into_iter().map(|index| segments[index]).collect(),
            bounds,
        });
    }
    result
}

fn topology_cell_is_inside_exclusive_maximum_bounds(cell: IVec2, min: IVec2, max: IVec2) -> bool {
    cell.cmpge(min).all() && cell.cmplt(max).all()
}

fn inclusive_topology_bounds_contain_point(rect: IRect, point: IVec2) -> bool {
    point.cmpge(rect.min).all() && point.cmple(rect.max).all()
}
