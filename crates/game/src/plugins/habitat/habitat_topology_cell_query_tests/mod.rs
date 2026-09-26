use bevy::prelude::*;

use super::habitat_topology_cell_queries::{
    convert_finite_world_position_to_topology_cell, sorted_topology_cells_contain_cell,
    topology_cells_overlap_inclusive_bounds,
};

#[test]
fn topology_cell_queries_cover_negative_world_space_and_inclusive_bounds() {
    let topology_cells = [IVec2::new(-2, 3), IVec2::new(-1, 3), IVec2::new(-1, 4)];
    assert_eq!(
        convert_finite_world_position_to_topology_cell(Vec2::new(-0.1, 2.9), 1.0),
        Some(IVec2::new(-1, 2)),
    );
    assert_eq!(
        convert_finite_world_position_to_topology_cell(Vec2::NAN, 1.0),
        None,
    );
    assert_eq!(
        convert_finite_world_position_to_topology_cell(Vec2::ZERO, -1.0),
        None,
    );
    assert!(sorted_topology_cells_contain_cell(
        &topology_cells,
        IVec2::new(-1, 3),
    ));
    assert!(!sorted_topology_cells_contain_cell(
        &topology_cells,
        IVec2::new(-2, 4),
    ));
    assert!(topology_cells_overlap_inclusive_bounds(
        &topology_cells,
        IRect::from_corners(IVec2::new(-3, 4), IVec2::new(-1, 5)),
    ));
    assert!(!topology_cells_overlap_inclusive_bounds(
        &topology_cells,
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    ));
}
