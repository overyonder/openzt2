use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::transport_tours::transport_route_position_calculations::{
    step_transport_route_position, TrackTravelDistances,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::RoutePosition;

fn ten_metre_test_track_segment() -> TrackSegment {
    TrackSegment {
        definition: AssetId::from_key("track"),

        from_endpoint_index: 0,
        from_position: Vec3::ZERO,

        to_endpoint_index: 1,
        to_position: Vec3::X * 10.0,
        path_points: vec![Vec3::ZERO, Vec3::X * 10.0].into_boxed_slice(),
        travel_path_points: vec![Vec3::ZERO, Vec3::X * 10.0].into_boxed_slice(),
        length: 10.0,
    }
}

#[test]
fn forward_route_position_clamps_at_track_segment_end() {
    let track_segment = ten_metre_test_track_segment();
    let mut route_position = RoutePosition { distance: 9.75 };
    assert!(step_transport_route_position(
        &mut route_position,
        track_segment.length,
        0.5,
        false,
    ));
    assert_eq!(route_position.distance, 10.0);
    assert_eq!(
        TrackTravelDistances::from_validated_segment(&track_segment)
            .unwrap()
            .translation_and_tangent(&track_segment, route_position.distance)
            .0,
        Vec3::X * 10.0,
    );
}

#[test]
fn reverse_route_position_preserves_progress_and_clamps_at_track_segment_start() {
    let track_segment = ten_metre_test_track_segment();
    let mut route_position = RoutePosition { distance: 4.0 };
    assert!(!step_transport_route_position(
        &mut route_position,
        track_segment.length,
        1.5,
        true,
    ));
    assert_eq!(route_position.distance, 2.5);
    assert_eq!(
        TrackTravelDistances::from_validated_segment(&track_segment)
            .unwrap()
            .translation_and_tangent(&track_segment, route_position.distance)
            .0,
        Vec3::X * 2.5,
    );
    assert!(step_transport_route_position(
        &mut route_position,
        track_segment.length,
        3.0,
        true,
    ));
    assert_eq!(route_position.distance, 0.0);
}
