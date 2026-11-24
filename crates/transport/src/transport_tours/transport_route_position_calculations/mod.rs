use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;

use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::RoutePosition;

#[derive(Component, PartialEq)]
pub(super) struct TrackTravelDistances {
    spans: Box<[[f32; 2]]>,
    cable: Option<Box<[Vec3]>>,
}

impl TrackTravelDistances {
    pub(super) fn calculate(path: &[Vec3]) -> Option<(Self, f32)> {
        if path.len() < 2 || !path[0].is_finite() {
            return None;
        }
        let mut length = 0.0;
        let mut spans = Vec::with_capacity(path.len() - 1);
        for points in path.windows(2) {
            let squared = points[0].distance_squared(points[1]);
            if !squared.is_finite() || squared <= f32::EPSILON {
                return None;
            }
            let span = squared.sqrt();
            spans.push([length, span]);
            length += span;
        }
        length.is_finite().then(|| {
            (
                Self {
                    spans: spans.into_boxed_slice(),
                    cable: None,
                },
                length,
            )
        })
    }

    pub(super) fn from_sky_cable(points: Vec<Vec3>) -> Option<Self> {
        let (mut distances, _) = Self::calculate(&points)?;
        distances.cable = Some(points.into_boxed_slice());
        Some(distances)
    }

    pub(super) fn sky_cable(&self) -> Option<&[Vec3]> {
        self.cable.as_deref()
    }

    pub(super) fn travel_length(&self, segment: &TrackSegment) -> f32 {
        if self.cable.is_some() {
            self.spans.last().map_or(0.0, |[start, span]| start + span)
        } else {
            segment.length
        }
    }

    pub(super) fn from_segment_snapshot(segment: &TrackSegment) -> Option<Self> {
        if segment.from_endpoint_index >= 2
            || segment.to_endpoint_index >= 2
            || segment.path_points.first() != Some(&segment.from_position)
            || segment.path_points.last() != Some(&segment.to_position)
            || segment.travel_path_points.first() != Some(&segment.from_position)
            || segment.travel_path_points.last() != Some(&segment.to_position)
            || !segment.length.is_finite()
            || segment.length <= 0.0
        {
            return None;
        }
        let (distances, length) = Self::calculate(&segment.travel_path_points)?;
        ((length - segment.length).abs() <= f32::EPSILON * segment.length.max(1.0))
            .then_some(distances)
    }

    pub(super) fn translation_and_tangent(
        &self,
        segment: &TrackSegment,
        distance: f32,
    ) -> (Vec3, Vec3) {
        let distance = distance.clamp(0.0, self.travel_length(segment));
        let points = self.cable.as_deref().unwrap_or(&segment.travel_path_points);
        let span_index = self
            .spans
            .partition_point(|&[start, length]| start + length < distance);
        if let Some(&[start, length]) = self.spans.get(span_index) {
            let from = points[span_index];
            let to = points[span_index + 1];
            (
                from.lerp(to, ((distance - start) / length).clamp(0.0, 1.0)),
                to - from,
            )
        } else {
            (
                points[points.len() - 1],
                points[points.len() - 1] - points[points.len() - 2],
            )
        }
    }
}

pub(super) fn refresh_transport_track_travel_distances(
    mut commands: Commands,
    mut track: Query<
        (
            Entity,
            Ref<TrackSegment>,
            Option<&TrackProfile>,
            Option<&mut TrackTravelDistances>,
        ),
        Changed<TrackSegment>,
    >,
) {
    for (entity, segment, profile, cached) in &mut track {
        if profile.is_some_and(|profile| profile.kind == TransportationTrackKind::Sky) {
            continue;
        }
        if segment.is_added() && cached.is_some() {
            continue;
        }
        match (
            TrackTravelDistances::from_segment_snapshot(&segment),
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
