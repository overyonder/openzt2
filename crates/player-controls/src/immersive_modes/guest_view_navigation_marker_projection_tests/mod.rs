use bevy::prelude::*;

use super::guest_view_navigation_marker_projection::insert_navigation_position_among_nearest_guest_view_positions;

#[test]
fn nearest_guest_view_navigation_position_insertion_is_bounded_and_sorted() {
    let mut nearest_navigation_positions = [(f32::INFINITY, Vec3::ZERO); 4];
    for squared_distance_from_target in [5.0, 1.0, 4.0, 2.0, 3.0] {
        insert_navigation_position_among_nearest_guest_view_positions(
            &mut nearest_navigation_positions,
            squared_distance_from_target,
            Vec3::splat(squared_distance_from_target),
        );
    }
    assert_eq!(
        nearest_navigation_positions.map(|(squared_distance, _)| squared_distance),
        [1.0, 2.0, 3.0, 4.0]
    );
}
