use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackDefinition;

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::{
    transport_topology_types::{TrackSegment, TransportStation},
    transport_vehicle_types::TransportVehicle,
};

pub(crate) fn transport_station_capacity_is_valid(transport_station: &TransportStation) -> bool {
    transport_station.capacity > 0
        && transport_station.queued <= transport_station.capacity
        && transport_station.occupied <= transport_station.capacity
        && transport_station
            .queued
            .saturating_add(transport_station.occupied)
            <= transport_station.capacity
}

pub(crate) fn transport_track_segment_is_valid(track_segment: &TrackSegment) -> bool {
    track_segment.from != track_segment.to
        && track_segment.from_endpoint_index < 2
        && track_segment.to_endpoint_index < 2
        && track_segment.from_position.is_finite()
        && track_segment.to_position.is_finite()
        && track_segment.path_points.first() == Some(&track_segment.from_position)
        && track_segment.path_points.last() == Some(&track_segment.to_position)
        && track_segment.travel_path_points.first() == Some(&track_segment.from_position)
        && track_segment.travel_path_points.last() == Some(&track_segment.to_position)
        && track_segment.length.is_finite()
        && track_segment.length > 0.0
        && transport_track_path_length(&track_segment.travel_path_points).is_some_and(|length| {
            (length - track_segment.length).abs() <= f32::EPSILON * track_segment.length.max(1.0)
        })
}

pub(crate) fn transport_track_path_length(path_points: &[Vec3]) -> Option<f32> {
    (path_points.len() >= 2
        && path_points.iter().all(|point| point.is_finite())
        && path_points
            .windows(2)
            .all(|points| points[0].distance_squared(points[1]) > f32::EPSILON))
    .then(|| {
        path_points
            .windows(2)
            .map(|points| points[0].distance(points[1]))
            .sum::<f32>()
    })
    .filter(|length| length.is_finite() && *length > 0.0)
}

pub(crate) fn authored_transport_track_piece_chain_is_valid(
    definition: &TransportationTrackDefinition,
    path_points: &[Vec3],
) -> bool {
    const TRACK_ALIGNMENT_EPSILON_METRES: f32 = 0.001;

    let source_endpoint_position = |source_position| {
        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
            source_position,
        ))
    };
    let horizontal_span = |endpoint_offsets: [[f32; 3]; 2]| {
        (source_endpoint_position(endpoint_offsets[1])
            - source_endpoint_position(endpoint_offsets[0]))
        .xz()
        .length()
    };
    let cardinal_piece_span = horizontal_span(definition.cardinal_endpoint_offsets_metres);
    let diagonal_piece_span = horizontal_span(definition.diagonal_endpoint_offsets_metres);
    cardinal_piece_span.is_finite()
        && cardinal_piece_span > TRACK_ALIGNMENT_EPSILON_METRES
        && diagonal_piece_span.is_finite()
        && diagonal_piece_span > TRACK_ALIGNMENT_EPSILON_METRES
        && transport_track_path_length(path_points).is_some()
        && path_points.windows(2).all(|points| {
            let displacement = points[1] - points[0];
            let horizontal = displacement.xz();
            let x_is_zero = horizontal.x.abs() < TRACK_ALIGNMENT_EPSILON_METRES;
            let y_is_zero = horizontal.y.abs() < TRACK_ALIGNMENT_EPSILON_METRES;
            let diagonal =
                (horizontal.x.abs() - horizontal.y.abs()).abs() < TRACK_ALIGNMENT_EPSILON_METRES;
            let expected_span = if diagonal {
                diagonal_piece_span
            } else if x_is_zero ^ y_is_zero {
                cardinal_piece_span
            } else {
                return false;
            };
            (horizontal.length() - expected_span).abs() < TRACK_ALIGNMENT_EPSILON_METRES
                && transport_track_grade_is_within_authored_limit(
                    points[0],
                    points[1],
                    definition.max_grade_permille,
                )
        })
}

pub(crate) fn transport_track_grade_is_within_authored_limit(
    start_position: Vec3,
    end_position: Vec3,
    maximum_grade_permille: u16,
) -> bool {
    if !start_position.is_finite() || !end_position.is_finite() {
        return false;
    }
    let displacement = end_position - start_position;
    let horizontal_distance = displacement.xz().length();
    if horizontal_distance <= f32::EPSILON {
        return displacement.y.abs() <= f32::EPSILON;
    }
    displacement.y.abs() * 1000.0
        <= horizontal_distance * f32::from(maximum_grade_permille) + f32::EPSILON
}

pub(crate) fn transport_vehicle_capacity_and_speed_are_valid(
    transport_vehicle: &TransportVehicle,
) -> bool {
    transport_vehicle.seats > 0
        && transport_vehicle.occupied <= transport_vehicle.seats
        && transport_vehicle
            .maximum_speed_metres_per_second
            .is_finite()
        && transport_vehicle.maximum_speed_metres_per_second >= 0.0
}
