use bevy::prelude::*;

/// Converts one finite world-XZ position to the shared topology cell.
pub(super) fn convert_finite_world_position_to_topology_cell(
    world_position_xz: Vec2,
    topology_cell_size_metres: f32,
) -> Option<IVec2> {
    (world_position_xz.is_finite()
        && topology_cell_size_metres.is_finite()
        && topology_cell_size_metres > 0.0)
        .then(|| {
            (world_position_xz / topology_cell_size_metres)
                .floor()
                .as_ivec2()
        })
}

/// Tests one cell against the canonical sorted region storage.
pub(super) fn sorted_topology_cells_contain_cell(
    sorted_topology_cells: &[IVec2],
    topology_cell: IVec2,
) -> bool {
    sorted_topology_cells
        .binary_search_by_key(&topology_cell.to_array(), |value| value.to_array())
        .is_ok()
}

/// Tests whether any canonical region cell intersects an inclusive cell rectangle.
pub(super) fn topology_cells_overlap_inclusive_bounds(
    topology_cells: &[IVec2],
    inclusive_topology_bounds: IRect,
) -> bool {
    topology_cells.iter().any(|topology_cell| {
        topology_cell.cmpge(inclusive_topology_bounds.min).all()
            && topology_cell.cmple(inclusive_topology_bounds.max).all()
    })
}
