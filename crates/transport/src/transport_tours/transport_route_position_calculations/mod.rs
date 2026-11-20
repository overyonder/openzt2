use bevy::prelude::*;

use crate::transport_tours::transport_fact_validation::transport_track_segment_is_valid;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::RoutePosition;

#[derive(Component)]
pub(super) struct TrackTravelDistances(Box<[[f32; 2]]>);

impl TrackTravelDistances {
    pub(super) fn from_validated_segment(segment: &TrackSegment) -> Option<Self> {
        if !transport_track_segment_is_valid(segment) {
            return None;
        }
        let mut cumulative = 0.0;
        Some(Self(
            segment
                .travel_path_points
                .windows(2)
                .map(|points| {
                    let span = points[0].distance(points[1]);
                    let start_distance = cumulative;
                    cumulative += span;
                    [start_distance, span]
                })
                .collect(),
        ))
    }

    pub(super) fn translation_and_tangent(
        &self,
        segment: &TrackSegment,
        distance: f32,
    ) -> (Vec3, Vec3) {
        let distance = distance.clamp(0.0, segment.length);
        let span_index = self
            .0
            .partition_point(|&[start, length]| start + length < distance);
        if let Some(&[start, length]) = self.0.get(span_index) {
            let from = segment.travel_path_points[span_index];
            let to = segment.travel_path_points[span_index + 1];
            (
                from.lerp(to, ((distance - start) / length).clamp(0.0, 1.0)),
                to - from,
            )
        } else {
            let points = &segment.travel_path_points;
            (
                segment.to_position,
                points[points.len() - 1] - points[points.len() - 2],
            )
        }
    }
}

pub(super) fn refresh_transport_track_travel_distances(
    mut commands: Commands,
    mut track: Query<
        (Entity, &TrackSegment, Option<&mut TrackTravelDistances>),
        Changed<TrackSegment>,
    >,
) {
    for (entity, segment, cached) in &mut track {
        match (
            TrackTravelDistances::from_validated_segment(segment),
            cached,
        ) {
            (Some(distances), Some(mut cached)) => *cached = distances,
            (Some(distances), None) => {
                commands.entity(entity).insert(distances);
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<TrackTravelDistances>();
            }
            (None, None) => {}
        }
    }
}

pub(super) fn step_transport_route_position(
    route_position: &mut RoutePosition,
    length: f32,
    travel_distance: f32,
    reverse: bool,
) -> bool {
    if !travel_distance.is_finite() || travel_distance < 0.0 {
        return false;
    }
    if reverse {
        route_position.distance =
            (route_position.distance.min(length).max(0.0) - travel_distance).max(0.0);
        route_position.distance <= 0.0
    } else {
        route_position.distance = (route_position.distance.max(0.0) + travel_distance).min(length);
        route_position.distance >= length
    }
}

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
