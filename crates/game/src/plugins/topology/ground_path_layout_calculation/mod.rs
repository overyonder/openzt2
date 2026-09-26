use bevy::prelude::*;

pub(crate) fn calculate_snapped_ground_path_layout(
    cell: IVec3,
    authored_width_centimetres: u16,
    topology_cell_size_centimetres: u16,
) -> Option<(IVec3, i32, i32)> {
    if topology_cell_size_centimetres == 0
        || authored_width_centimetres % topology_cell_size_centimetres != 0
    {
        return None;
    }
    let width_in_topology_cells =
        i32::from(authored_width_centimetres / topology_cell_size_centimetres);
    if width_in_topology_cells == 0 || width_in_topology_cells % 2 != 0 {
        return None;
    }
    let placement_step_in_topology_cells = width_in_topology_cells.checked_mul(2)?;
    Some((
        IVec3::new(
            snap_topology_grid_axis_to_ground_path_lattice(
                cell.x,
                width_in_topology_cells,
                placement_step_in_topology_cells,
            )?,
            snap_topology_grid_axis_to_ground_path_lattice(
                cell.y,
                width_in_topology_cells,
                placement_step_in_topology_cells,
            )?,
            cell.z,
        ),
        width_in_topology_cells,
        placement_step_in_topology_cells,
    ))
}

pub(super) fn snap_topology_grid_axis_to_ground_path_lattice(
    coordinate: i32,
    width_in_topology_cells: i32,
    placement_step_in_topology_cells: i32,
) -> Option<i32> {
    let coordinate = i64::from(coordinate);
    let width_in_topology_cells = i64::from(width_in_topology_cells);
    let placement_step_in_topology_cells = i64::from(placement_step_in_topology_cells);
    let relative_coordinate = coordinate - width_in_topology_cells;
    let lower_coordinate = relative_coordinate
        .div_euclid(placement_step_in_topology_cells)
        .checked_mul(placement_step_in_topology_cells)?
        .checked_add(width_in_topology_cells)?;
    let upper_coordinate = lower_coordinate.checked_add(placement_step_in_topology_cells)?;
    i32::try_from(
        if coordinate - lower_coordinate <= upper_coordinate - coordinate {
            lower_coordinate
        } else {
            upper_coordinate
        },
    )
    .ok()
}
