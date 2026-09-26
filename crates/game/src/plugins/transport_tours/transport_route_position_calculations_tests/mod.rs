use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    transport_route_position_calculations::{
        advance_route_position_toward_segment_end,
        calculate_translation_along_transport_track_segment,
        retreat_route_position_toward_segment_start,
    },
    transport_topology_types::TrackSegment,
    transport_vehicle_types::RoutePosition,
};

fn ten_metre_test_track_segment() -> TrackSegment {
    TrackSegment {
        definition: AssetId::from_key("track"),
        from: Entity::from_bits(1),
        from_endpoint_index: 0,
        from_position: Vec3::ZERO,
        to: Entity::from_bits(2),
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
    let mut route_position = RoutePosition {
        segment: Entity::from_bits(3),
        distance: 9.75,
    };
    assert!(advance_route_position_toward_segment_end(
        &mut route_position,
        &track_segment,
        0.5,
    ));
    assert_eq!(route_position.distance, 10.0);
    assert_eq!(
        calculate_translation_along_transport_track_segment(
            &track_segment,
            route_position.distance,
        ),
        Vec3::X * 10.0,
    );
}

#[test]
fn reverse_route_position_preserves_progress_and_clamps_at_track_segment_start() {
    let track_segment = ten_metre_test_track_segment();
    let mut route_position = RoutePosition {
        segment: Entity::from_bits(3),
        distance: 4.0,
    };
    assert!(!retreat_route_position_toward_segment_start(
        &mut route_position,
        &track_segment,
        1.5,
    ));
    assert_eq!(route_position.distance, 2.5);
    assert_eq!(
        calculate_translation_along_transport_track_segment(
            &track_segment,
            route_position.distance,
        ),
        Vec3::X * 2.5,
    );
    assert!(retreat_route_position_toward_segment_start(
        &mut route_position,
        &track_segment,
        3.0,
    ));
    assert_eq!(route_position.distance, 0.0);
}
