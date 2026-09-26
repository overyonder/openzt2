use bevy::prelude::IVec3;

use crate::plugins::construction::construction_interaction_types::PlacementFailure;

use super::topology_grid_geometry::{
    calculate_adjacent_topology_grid_cells_between_endpoints,
    calculate_heading_quantized_fence_segment_cells_between_endpoints,
};

#[test]
fn line_expansion_is_deterministic_and_adjacent() {
    let forward =
        calculate_adjacent_topology_grid_cells_between_endpoints(IVec3::ZERO, IVec3::new(4, 0, 2))
            .unwrap();
    let mut reverse =
        calculate_adjacent_topology_grid_cells_between_endpoints(IVec3::new(4, 0, 2), IVec3::ZERO)
            .unwrap();
    reverse.reverse();
    assert_eq!(forward, reverse);
    assert_eq!(forward.first(), Some(&IVec3::ZERO));
    assert_eq!(forward.last(), Some(&IVec3::new(4, 0, 2)));
    assert!(forward.windows(2).all(|pair| {
        let delta = pair[1] - pair[0];
        delta.x.abs() <= 1 && delta.y.abs() <= 1 && delta.z.abs() <= 1
    }));
}

#[test]
fn excessive_elevation_change_is_rejected() {
    assert_eq!(
        calculate_adjacent_topology_grid_cells_between_endpoints(IVec3::ZERO, IVec3::new(1, 0, 4),),
        Err(PlacementFailure::NoHeadroom)
    );
}

#[test]
fn fence_segment_walk_recomputes_the_nearest_45_degree_heading_after_each_segment() {
    assert_eq!(
        calculate_heading_quantized_fence_segment_cells_between_endpoints(
            IVec3::ZERO,
            IVec3::new(12, 6, 0),
            3,
        )
        .unwrap(),
        vec![
            IVec3::ZERO,
            IVec3::new(3, 3, 0),
            IVec3::new(6, 3, 0),
            IVec3::new(9, 6, 0),
            IVec3::new(12, 6, 0),
        ]
    );
}
