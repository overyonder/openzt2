use bevy::{math::DVec2, prelude::*};

use crate::plugins::construction::construction_interaction_types::PlacementFailure;

use super::topology_graph_types::TopologyGrid;

pub(crate) fn calculate_adjacent_topology_grid_cells_between_endpoints(
    from: IVec3,
    to: IVec3,
) -> Result<Vec<IVec3>, PlacementFailure> {
    let coordinate_differences = [
        i64::from(to.x) - i64::from(from.x),
        i64::from(to.y) - i64::from(from.y),
        i64::from(to.z) - i64::from(from.z),
    ];
    let step_count = coordinate_differences[0]
        .unsigned_abs()
        .max(coordinate_differences[1].unsigned_abs());
    if step_count == 0 {
        return (from.z == to.z)
            .then(|| vec![from])
            .ok_or(PlacementFailure::NoHeadroom);
    }
    let capacity = usize::try_from(step_count)
        .ok()
        .and_then(|step_count| step_count.checked_add(1))
        .ok_or(PlacementFailure::InvalidTopology)?;
    let mut cells = Vec::with_capacity(capacity);
    for step in 0..=step_count {
        let interpolation = step as f64 / step_count as f64;
        let cell = IVec3::new(
            (f64::from(from.x) + coordinate_differences[0] as f64 * interpolation).round() as i32,
            (f64::from(from.y) + coordinate_differences[1] as f64 * interpolation).round() as i32,
            (f64::from(from.z) + coordinate_differences[2] as f64 * interpolation).round() as i32,
        );
        if cells.last().copied() != Some(cell) {
            cells.push(cell);
        }
    }
    if cells.windows(2).any(|pair| {
        let coordinate_difference = pair[1] - pair[0];
        coordinate_difference.x.abs() > 1
            || coordinate_difference.y.abs() > 1
            || coordinate_difference.z.abs() > 1
    }) {
        return Err(PlacementFailure::NoHeadroom);
    }
    Ok(cells)
}

/// Reproduces the fence tool's authored rubber-band walk. Each fence point is
/// one complete segment from the previous point. The heading toward the live
/// pointer endpoint is recomputed after every segment, rounded to the nearest
/// 45-degree direction, and advanced by one authored segment span. A remainder
/// shorter than one segment is deliberately left unbuilt.
pub(crate) fn calculate_heading_quantized_fence_segment_cells_between_endpoints(
    from: IVec3,
    to: IVec3,
    segment_cell_span: u16,
) -> Result<Vec<IVec3>, PlacementFailure> {
    if from.z != to.z || segment_cell_span == 0 {
        return Err(PlacementFailure::NoHeadroom);
    }
    let segment_cell_span = i32::from(segment_cell_span);
    let horizontal_difference = [
        i64::from(to.x) - i64::from(from.x),
        i64::from(to.y) - i64::from(from.y),
    ];
    let complete_segment_count = horizontal_difference
        .into_iter()
        .map(i64::unsigned_abs)
        .max()
        .unwrap_or_default()
        / u64::try_from(segment_cell_span).map_err(|_| PlacementFailure::InvalidTopology)?;
    if complete_segment_count == 0 {
        return Ok(vec![from]);
    }
    let required_capacity = usize::try_from(complete_segment_count)
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or(PlacementFailure::InvalidTopology)?;
    let mut cells = Vec::with_capacity(required_capacity);
    cells.push(from);
    let quantized_heading_directions = [
        IVec2::X,
        IVec2::ONE,
        IVec2::Y,
        IVec2::new(-1, 1),
        IVec2::NEG_X,
        IVec2::NEG_ONE,
        IVec2::NEG_Y,
        IVec2::new(1, -1),
    ];
    for _ in 0..required_capacity - 1 {
        let current = *cells
            .last()
            .expect("the authored fence walk starts at its anchor");
        let difference = DVec2::new(
            f64::from(to.x) - f64::from(current.x),
            f64::from(to.y) - f64::from(current.y),
        );
        if difference == DVec2::ZERO {
            break;
        }
        let normalized_difference = difference.normalize();
        let direction = quantized_heading_directions
            .iter()
            .max_by(|left, right| {
                left.as_vec2()
                    .normalize()
                    .as_dvec2()
                    .dot(normalized_difference)
                    .total_cmp(
                        &right
                            .as_vec2()
                            .normalize()
                            .as_dvec2()
                            .dot(normalized_difference),
                    )
            })
            .copied()
            .ok_or(PlacementFailure::InvalidTopology)?;
        let next_horizontal = current.xy() + direction * segment_cell_span;
        cells.push(IVec3::new(next_horizontal.x, next_horizontal.y, from.z));
    }
    Ok(cells)
}

pub(super) fn calculate_topology_edge_cell_bounds(first: IVec3, second: IVec3) -> IRect {
    IRect::from_corners(
        IVec2::new(first.x.min(second.x), first.y.min(second.y)),
        IVec2::new(first.x.max(second.x), first.y.max(second.y)),
    )
}

pub(crate) fn calculate_topology_edge_world_transform(
    first: IVec3,
    second: IVec3,
    grid: TopologyGrid,
) -> Transform {
    let first_translation = grid.cell_translation(first);
    let second_translation = grid.cell_translation(second);
    let translation_difference = second_translation - first_translation;
    // Shipped fence meshes begin at their local origin and extend along +X;
    // the inherited BFSkewComponent confirms that authored axis as `0`.
    Transform::from_translation(first_translation).with_rotation(Quat::from_rotation_y(
        (-translation_difference.z).atan2(translation_difference.x),
    ))
}

pub(super) fn merge_topology_changed_cell_bounds(
    current: Option<IRect>,
    next: IRect,
) -> Option<IRect> {
    Some(current.map_or(next, |current| current.union(next)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_edge_transform_places_local_x_between_authored_endpoints() {
        let grid = TopologyGrid {
            origin: Vec2::new(4.0, -7.0),
            spacing_m: 3.0,
        };
        for (first, second) in [
            (IVec3::new(1, 2, 0), IVec3::new(2, 2, 0)),
            (IVec3::new(1, 2, 0), IVec3::new(1, 3, 0)),
            (IVec3::new(1, 2, 0), IVec3::new(2, 3, 0)),
            (IVec3::new(1, 2, 0), IVec3::new(0, 1, 0)),
        ] {
            let first_world = grid.cell_translation(first);
            let second_world = grid.cell_translation(second);
            let transform = calculate_topology_edge_world_transform(first, second, grid);
            let segment_length = first_world.distance(second_world);

            assert!(transform
                .transform_point(Vec3::ZERO)
                .abs_diff_eq(first_world, 0.000_01));
            assert!(transform
                .transform_point(Vec3::X * segment_length)
                .abs_diff_eq(second_world, 0.000_01));
        }
    }
}
