use bevy::prelude::*;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use crate::habitat::habitat_terrain_summary_refresh::calculate_topology_cell_bounds_affected_by_terrain_change;

#[test]
fn terrain_edit_bounds_include_adjacent_bilinear_sampled_topology_cells() {
    let terrain_chunk = openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk {
        source_coord: IVec2::ZERO,
        coord: IVec2::ZERO,
        asset: default(),
        origin: Vec2::new(-8.0, 12.0),
        spacing_m: 2.0,
        side: 17,
    };
    assert_eq!(
        calculate_topology_cell_bounds_affected_by_terrain_change(
            &terrain_chunk,
            TopologyGrid {
                origin: Vec2::new(-8.0, 12.0),
                spacing_m: 2.0
            },
            UVec2::new(2, 3),
            UVec2::new(5, 7),
        ),
        Some(IRect::from_corners(IVec2::new(1, 2), IVec2::new(6, 8),)),
    );
    assert_eq!(
        calculate_topology_cell_bounds_affected_by_terrain_change(
            &terrain_chunk,
            TopologyGrid::default(),
            UVec2::ONE,
            UVec2::ZERO,
        ),
        None,
    );
}
