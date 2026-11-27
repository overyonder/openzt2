use bevy::prelude::*;

use crate::topology::topology_graph_types::TopologyGrid;

use crate::habitat::habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell;
use crate::habitat::habitat_types::{HabitatCellOf, HabitatIndex, HabitatSector};

/// Locates the habitat containing one finite world-XZ position.
pub fn locate_habitat_at_world_position(
    habitat_index: &HabitatIndex,
    cell_owners: &Query<&HabitatCellOf>,
    world_position_xz: Vec2,
    topology_grid: TopologyGrid,
) -> Option<Entity> {
    let local = world_position_xz - topology_grid.origin;
    let coordinate =
        convert_finite_world_position_to_topology_cell(local, topology_grid.spacing_m)?;
    let centre = (coordinate.as_vec2() + Vec2::splat(0.5)) * topology_grid.spacing_m;
    let sector = HabitatSector::from_centre_offset(local - centre);
    let cell = habitat_index.cell_in_sector(coordinate, sector)?;
    cell_owners.get(cell).ok().map(|owner| owner.0)
}

pub fn locate_habitat_at_topology_cell(
    habitat_index: &HabitatIndex,
    cell_owners: &Query<&HabitatCellOf>,
    coordinate: IVec2,
) -> Option<Entity> {
    let cell = habitat_index.cell_at(coordinate)?;
    cell_owners.get(cell).ok().map(|owner| owner.0)
}
