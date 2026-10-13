use bevy::prelude::*;

use crate::topology::topology_graph_types::TopologyGrid;

use crate::habitat::habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell;
use crate::habitat::habitat_types::HabitatIndex;

/// Locates the habitat containing one finite world-XZ position.
pub fn locate_habitat_at_world_position(
    habitat_index: &HabitatIndex,
    world_position_xz: Vec2,
    topology_grid: TopologyGrid,
) -> Option<Entity> {
    convert_finite_world_position_to_topology_cell(
        world_position_xz - topology_grid.origin,
        topology_grid.spacing_m,
    )
    .and_then(|topology_cell| habitat_index.habitat_for_topology_cell(topology_cell))
}
