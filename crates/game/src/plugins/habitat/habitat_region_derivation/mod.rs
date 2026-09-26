use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct HabitatFenceSegment {
    pub fence_entity: Entity,
    pub first_topology_node: IVec2,
    pub second_topology_node: IVec2,
    pub boundary_is_breached: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct DerivedHabitatRegion {
    pub topology_cells: Vec<IVec2>,
    pub boundary_fence_entities: Vec<Entity>,
    pub boundary_is_breached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TopologyCellPassage {
    lower_topology_cell: IVec2,
    upper_topology_cell: IVec2,
}

impl TopologyCellPassage {
    fn new(first_topology_cell: IVec2, second_topology_cell: IVec2) -> Self {
        if topology_cell_sort_key(first_topology_cell)
            <= topology_cell_sort_key(second_topology_cell)
        {
            Self {
                lower_topology_cell: first_topology_cell,
                upper_topology_cell: second_topology_cell,
            }
        } else {
            Self {
                lower_topology_cell: second_topology_cell,
                upper_topology_cell: first_topology_cell,
            }
        }
    }
}

pub(super) fn derive_enclosed_habitat_regions_from_fence_segments(
    fence_segments: &[HabitatFenceSegment],
    changed_topology_bounds: IRect,
) -> Vec<DerivedHabitatRegion> {
    let expanded =
        expand_authored_fence_segments_into_cardinal_habitat_grid_boundaries(fence_segments);
    if expanded.is_empty() {
        return Vec::new();
    }
    let relevant = collect_connected_fence_segments_intersecting_changed_bounds(
        &expanded,
        changed_topology_bounds,
    );
    if relevant.is_empty() {
        return Vec::new();
    }

    let mut min = IVec2::splat(i32::MAX);
    let mut max = IVec2::splat(i32::MIN);
    let mut blocked = HashMap::with_capacity(relevant.len() * 2);
    for segment in &relevant {
        min = min
            .min(segment.first_topology_node)
            .min(segment.second_topology_node);
        max = max
            .max(segment.first_topology_node)
            .max(segment.second_topology_node);
        if let Some((left, right)) = find_topology_cells_separated_by_fence_segment(
            segment.first_topology_node,
            segment.second_topology_node,
        ) {
            blocked.insert(TopologyCellPassage::new(left, right), segment.fence_entity);
        }
    }
    min -= IVec2::ONE;
    max += IVec2::ONE;

    let mut outside = HashSet::with_capacity(calculate_exclusive_maximum_topology_rectangle_area(
        min, max,
    ));
    let mut queue = Vec::with_capacity(calculate_exclusive_maximum_topology_rectangle_area(
        min, max,
    ));
    for x in min.x..max.x {
        append_topology_cell_if_not_seen(IVec2::new(x, min.y), &mut outside, &mut queue);
        append_topology_cell_if_not_seen(IVec2::new(x, max.y - 1), &mut outside, &mut queue);
    }
    for y in min.y..max.y {
        append_topology_cell_if_not_seen(IVec2::new(min.x, y), &mut outside, &mut queue);
        append_topology_cell_if_not_seen(IVec2::new(max.x - 1, y), &mut outside, &mut queue);
    }
    let mut cursor = 0;
    while let Some(&cell) = queue.get(cursor) {
        cursor += 1;
        for neighbor in four_cardinal_topology_cell_neighbors(cell) {
            if topology_cell_is_inside_exclusive_maximum_bounds(neighbor, min, max)
                && !blocked.contains_key(&TopologyCellPassage::new(cell, neighbor))
            {
                append_topology_cell_if_not_seen(neighbor, &mut outside, &mut queue);
            }
        }
    }

    let mut interiors = HashSet::new();
    for y in min.y..max.y {
        for x in min.x..max.x {
            let cell = IVec2::new(x, y);
            if !outside.contains(&cell) {
                interiors.insert(cell);
            }
        }
    }

    let mut regions = Vec::new();
    while let Some(&start) = interiors
        .iter()
        .min_by_key(|cell| topology_cell_sort_key(**cell))
    {
        interiors.remove(&start);
        let mut cells = vec![start];
        let mut at = 0;
        while let Some(&cell) = cells.get(at) {
            at += 1;
            for neighbor in four_cardinal_topology_cell_neighbors(cell) {
                if !blocked.contains_key(&TopologyCellPassage::new(cell, neighbor))
                    && interiors.remove(&neighbor)
                {
                    cells.push(neighbor);
                }
            }
        }
        cells.sort_unstable_by_key(|cell| topology_cell_sort_key(*cell));
        let cell_set: HashSet<_> = cells.iter().copied().collect();
        let mut boundary_edges = Vec::new();
        let mut breached = false;
        for segment in &relevant {
            let Some((left, right)) = find_topology_cells_separated_by_fence_segment(
                segment.first_topology_node,
                segment.second_topology_node,
            ) else {
                continue;
            };
            if cell_set.contains(&left) ^ cell_set.contains(&right) {
                boundary_edges.push(segment.fence_entity);
                breached |= segment.boundary_is_breached;
            }
        }
        boundary_edges.sort_unstable_by_key(|entity| entity.to_bits());
        boundary_edges.dedup();
        regions.push(DerivedHabitatRegion {
            topology_cells: cells,
            boundary_fence_entities: boundary_edges,
            boundary_is_breached: breached,
        });
    }
    regions.sort_by_key(|region| {
        region
            .topology_cells
            .first()
            .copied()
            .map(topology_cell_sort_key)
    });
    regions
}

fn expand_authored_fence_segments_into_cardinal_habitat_grid_boundaries(
    segments: &[HabitatFenceSegment],
) -> Vec<HabitatFenceSegment> {
    let mut result = Vec::new();
    for segment in segments {
        // Canonical direction makes the cell-boundary raster independent of
        // which way the player drew the fence. This is the habitat's existing
        // whole-cell approximation; rendered and collision geometry stay diagonal.
        let (start, end) = if topology_cell_sort_key(segment.first_topology_node)
            <= topology_cell_sort_key(segment.second_topology_node)
        {
            (segment.first_topology_node, segment.second_topology_node)
        } else {
            (segment.second_topology_node, segment.first_topology_node)
        };
        let delta = end - start;
        if (delta.x != 0 && delta.y != 0 && delta.x.unsigned_abs() != delta.y.unsigned_abs())
            || delta == IVec2::ZERO
        {
            continue;
        }
        let step = delta.signum();
        let length = delta.x.unsigned_abs().max(delta.y.unsigned_abs());
        for offset in 0..length {
            let first = start + step * offset as i32;
            let corner = first + IVec2::new(step.x, 0);
            let second = first + step;
            for (first_topology_node, second_topology_node) in [(first, corner), (corner, second)] {
                if first_topology_node != second_topology_node {
                    result.push(HabitatFenceSegment {
                        fence_entity: segment.fence_entity,
                        first_topology_node,
                        second_topology_node,
                        boundary_is_breached: segment.boundary_is_breached,
                    });
                }
            }
        }
    }
    result
}

fn collect_connected_fence_segments_intersecting_changed_bounds(
    segments: &[HabitatFenceSegment],
    changed: IRect,
) -> Vec<HabitatFenceSegment> {
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
        let mut relevant = false;
        while let Some(&edge_index) = component.get(cursor) {
            cursor += 1;
            let edge = segments[edge_index];
            relevant |= inclusive_topology_bounds_contain_point(changed, edge.first_topology_node)
                || inclusive_topology_bounds_contain_point(changed, edge.second_topology_node);
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
        if relevant {
            result.extend(component.into_iter().map(|index| segments[index]));
        }
    }
    result
}

fn find_topology_cells_separated_by_fence_segment(a: IVec2, b: IVec2) -> Option<(IVec2, IVec2)> {
    let delta = b - a;
    match (delta.x, delta.y) {
        (1 | -1, 0) => {
            let x = a.x.min(b.x);
            Some((IVec2::new(x, a.y - 1), IVec2::new(x, a.y)))
        }
        (0, 1 | -1) => {
            let y = a.y.min(b.y);
            Some((IVec2::new(a.x - 1, y), IVec2::new(a.x, y)))
        }
        _ => None,
    }
}

fn four_cardinal_topology_cell_neighbors(cell: IVec2) -> [IVec2; 4] {
    [
        cell + IVec2::X,
        cell - IVec2::X,
        cell + IVec2::Y,
        cell - IVec2::Y,
    ]
}

fn append_topology_cell_if_not_seen(
    cell: IVec2,
    seen: &mut HashSet<IVec2>,
    queue: &mut Vec<IVec2>,
) {
    if seen.insert(cell) {
        queue.push(cell);
    }
}

fn topology_cell_is_inside_exclusive_maximum_bounds(cell: IVec2, min: IVec2, max: IVec2) -> bool {
    cell.cmpge(min).all() && cell.cmplt(max).all()
}

fn inclusive_topology_bounds_contain_point(rect: IRect, point: IVec2) -> bool {
    point.cmpge(rect.min).all() && point.cmple(rect.max).all()
}

fn calculate_exclusive_maximum_topology_rectangle_area(min: IVec2, max: IVec2) -> usize {
    let side = (max - min).max(IVec2::ZERO);
    usize::try_from(i64::from(side.x) * i64::from(side.y)).unwrap_or(0)
}

fn topology_cell_sort_key(cell: IVec2) -> (i32, i32) {
    (cell.x, cell.y)
}
