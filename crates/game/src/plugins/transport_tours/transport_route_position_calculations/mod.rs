use bevy::prelude::*;

use super::{
    transport_fact_validation::transport_track_segment_is_valid,
    transport_topology_types::TrackSegment, transport_vehicle_types::RoutePosition,
};

pub(super) fn construct_native_ground_transport_track_travel_path_points(
    construction_points: &[Vec3],
) -> Box<[Vec3]> {
    let Some(first) = construction_points.first().copied() else {
        return Box::default();
    };
    let mut result = Vec::with_capacity(construction_points.len().saturating_mul(5));
    result.push(first);
    for index in 0..construction_points.len().saturating_sub(1) {
        let start = construction_points[index];
        let end = construction_points[index + 1];
        let displacement = end - start;
        let diagonal_or_elevated = displacement.y.abs() > f32::EPSILON
            || (displacement.x.abs() > f32::EPSILON && displacement.z.abs() > f32::EPSILON);
        if diagonal_or_elevated {
            result.push(end);
            continue;
        }
        let start_tangent = if index == 0 {
            displacement
        } else {
            end - construction_points[index - 1]
        }
        .normalize_or_zero();
        let end_tangent = construction_points
            .get(index + 2)
            .map_or(displacement, |next| *next - start)
            .normalize_or_zero();
        for sample in 1..=5 {
            let t = sample as f32 / 5.0;
            let squared = t * t;
            let cubed = squared * t;
            result.push(
                start * (2.0 * cubed - 3.0 * squared + 1.0)
                    + end * (-2.0 * cubed + 3.0 * squared)
                    + start_tangent * (cubed - 2.0 * squared + t)
                    + end_tangent * (cubed - squared),
            );
        }
    }
    result.into_boxed_slice()
}

pub(crate) fn advance_route_position_toward_segment_end(
    route_position: &mut RoutePosition,
    track_segment: &TrackSegment,
    travel_distance: f32,
) -> bool {
    if !travel_distance.is_finite()
        || travel_distance < 0.0
        || !transport_track_segment_is_valid(track_segment)
    {
        return false;
    }
    route_position.distance =
        (route_position.distance.max(0.0) + travel_distance).min(track_segment.length);
    route_position.distance >= track_segment.length
}

pub(crate) fn retreat_route_position_toward_segment_start(
    route_position: &mut RoutePosition,
    track_segment: &TrackSegment,
    travel_distance: f32,
) -> bool {
    if !travel_distance.is_finite()
        || travel_distance < 0.0
        || !transport_track_segment_is_valid(track_segment)
    {
        return false;
    }
    route_position.distance =
        (route_position.distance.min(track_segment.length).max(0.0) - travel_distance).max(0.0);
    route_position.distance <= 0.0
}

pub(crate) fn calculate_translation_along_transport_track_segment(
    track_segment: &TrackSegment,
    distance_along_segment: f32,
) -> Vec3 {
    let mut remaining_distance = distance_along_segment.clamp(0.0, track_segment.length);
    for points in track_segment.travel_path_points.windows(2) {
        let piece_length = points[0].distance(points[1]);
        if remaining_distance <= piece_length {
            return points[0].lerp(
                points[1],
                (remaining_distance / piece_length).clamp(0.0, 1.0),
            );
        }
        remaining_distance -= piece_length;
    }
    track_segment.to_position
}

pub(crate) fn calculate_forward_displacement_along_transport_track_segment(
    track_segment: &TrackSegment,
    distance_along_segment: f32,
) -> Vec3 {
    let mut remaining_distance = distance_along_segment.clamp(0.0, track_segment.length);
    for points in track_segment.travel_path_points.windows(2) {
        let piece_length = points[0].distance(points[1]);
        if remaining_distance <= piece_length {
            return points[1] - points[0];
        }
        remaining_distance -= piece_length;
    }
    track_segment
        .travel_path_points
        .windows(2)
        .next_back()
        .map_or(Vec3::ZERO, |points| points[1] - points[0])
}
