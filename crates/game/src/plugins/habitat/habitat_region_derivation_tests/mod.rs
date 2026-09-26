use bevy::prelude::*;

use super::habitat_region_derivation::{
    derive_enclosed_habitat_regions_from_fence_segments, HabitatFenceSegment,
};

#[test]
fn diagonal_enclosure_is_closed_in_both_draw_directions_and_opens_when_a_side_is_removed() {
    let corners = [
        IVec2::new(0, 2),
        IVec2::new(2, 0),
        IVec2::new(0, -2),
        IVec2::new(-2, 0),
    ];
    let mut segments: Vec<_> = (0..4)
        .map(|index| HabitatFenceSegment {
            fence_entity: Entity::from_bits((index + 1) as u64),
            first_topology_node: corners[index],
            second_topology_node: corners[(index + 1) % 4],
            boundary_is_breached: false,
        })
        .collect();
    let bounds = IRect::from_corners(IVec2::splat(-2), IVec2::splat(2));
    let closed = derive_enclosed_habitat_regions_from_fence_segments(&segments, bounds);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].topology_cells.len(), 8);
    assert_eq!(closed[0].boundary_fence_entities.len(), 4);
    for segment in &mut segments {
        std::mem::swap(
            &mut segment.first_topology_node,
            &mut segment.second_topology_node,
        );
    }
    assert_eq!(
        derive_enclosed_habitat_regions_from_fence_segments(&segments, bounds),
        closed
    );
    segments.pop();
    assert!(derive_enclosed_habitat_regions_from_fence_segments(&segments, bounds).is_empty());
}

fn create_square_habitat_fence_segments(
    topology_origin: IVec2,
    side_length_in_topology_cells: i32,
    breached_fence_segment_index: Option<usize>,
) -> Vec<HabitatFenceSegment> {
    let mut fence_segments = Vec::new();
    let mut append_fence_segment = |first_topology_node, second_topology_node| {
        let fence_segment_index = fence_segments.len();
        fence_segments.push(HabitatFenceSegment {
            fence_entity: Entity::from_bits((fence_segment_index + 1) as u64),
            first_topology_node: first_topology_node + topology_origin,
            second_topology_node: second_topology_node + topology_origin,
            boundary_is_breached: breached_fence_segment_index == Some(fence_segment_index),
        });
    };
    for topology_x in 0..side_length_in_topology_cells {
        append_fence_segment(IVec2::new(topology_x, 0), IVec2::new(topology_x + 1, 0));
        append_fence_segment(
            IVec2::new(topology_x, side_length_in_topology_cells),
            IVec2::new(topology_x + 1, side_length_in_topology_cells),
        );
    }
    for topology_y in 0..side_length_in_topology_cells {
        append_fence_segment(IVec2::new(0, topology_y), IVec2::new(0, topology_y + 1));
        append_fence_segment(
            IVec2::new(side_length_in_topology_cells, topology_y),
            IVec2::new(side_length_in_topology_cells, topology_y + 1),
        );
    }
    fence_segments
}

#[test]
fn closed_fence_region_is_deterministic_and_open_gate_marks_boundary_as_breached() {
    let closed_region = derive_enclosed_habitat_regions_from_fence_segments(
        &create_square_habitat_fence_segments(IVec2::ZERO, 2, None),
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    );
    assert_eq!(closed_region.len(), 1);
    assert_eq!(
        closed_region[0].topology_cells,
        [
            IVec2::new(0, 0),
            IVec2::new(0, 1),
            IVec2::new(1, 0),
            IVec2::new(1, 1),
        ],
    );
    assert!(!closed_region[0].boundary_is_breached);

    let open_region = derive_enclosed_habitat_regions_from_fence_segments(
        &create_square_habitat_fence_segments(IVec2::ZERO, 2, Some(0)),
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    );
    assert_eq!(open_region.len(), 1);
    assert!(open_region[0].boundary_is_breached);
}

#[test]
fn missing_fence_opens_region_and_internal_divider_splits_region_until_removed() {
    let mut broken_boundary = create_square_habitat_fence_segments(IVec2::ZERO, 2, None);
    broken_boundary.pop();
    assert!(derive_enclosed_habitat_regions_from_fence_segments(
        &broken_boundary,
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    )
    .is_empty());

    let mut divided_boundary = create_square_habitat_fence_segments(IVec2::ZERO, 2, None);
    divided_boundary.push(HabitatFenceSegment {
        fence_entity: Entity::from_bits(100),
        first_topology_node: IVec2::new(1, 0),
        second_topology_node: IVec2::new(1, 2),
        boundary_is_breached: false,
    });
    let split_regions = derive_enclosed_habitat_regions_from_fence_segments(
        &divided_boundary,
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    );
    assert_eq!(split_regions.len(), 2);
    assert_eq!(split_regions[0].topology_cells.len(), 2);
    assert_eq!(split_regions[1].topology_cells.len(), 2);

    let merged_region = derive_enclosed_habitat_regions_from_fence_segments(
        &create_square_habitat_fence_segments(IVec2::ZERO, 2, None),
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    );
    assert_eq!(merged_region.len(), 1);
    assert_eq!(merged_region[0].topology_cells.len(), 4);
}

#[test]
fn changed_bounds_select_only_the_connected_fence_enclosure_they_intersect() {
    let mut fence_segments = create_square_habitat_fence_segments(IVec2::ZERO, 2, None);
    fence_segments.extend(create_square_habitat_fence_segments(
        IVec2::new(10, 10),
        3,
        None,
    ));
    let derived_regions = derive_enclosed_habitat_regions_from_fence_segments(
        &fence_segments,
        IRect::from_corners(IVec2::ZERO, IVec2::splat(2)),
    );
    assert_eq!(derived_regions.len(), 1);
    assert_eq!(derived_regions[0].topology_cells.len(), 4);
}
