use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::prelude::*;

use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;

use crate::transport_tours::transport_route_position_calculations::{
    step_transport_route_position, TrackTravelDistances,
};
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitDirection;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitRunning;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::RoutePosition;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::StationDestination;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::{
    TransportVehicle, TransportVehicleArrival,
};

pub(super) fn advance_transport_vehicles_along_canonical_track_segments(
    spatial_query: SpatialQuery,
    fixed_time: Res<Time<Fixed>>,
    mut transport_vehicles: Query<(
        Entity,
        &TransportVehicle,
        &mut RoutePosition,
        &mut StationDestination,
        &mut Transform,
        &mut Velocity,
        &CircuitMember,
        Option<&DirectLocomotion>,
        &mut TransportVehicleArrival,
    )>,
    track_segments: Query<(Entity, &TrackSegment, &CircuitMember)>,
    track_travel_distances: Query<&TrackTravelDistances, With<TrackSegment>>,
    transport_stations: Query<(), With<TransportStation>>,
    entity_parents: Query<&ChildOf>,
    transport_circuits: Query<(
        &TransportCircuit,
        Option<&CircuitRunning>,
        Option<&CircuitDirection>,
    )>,
) {
    let fixed_delta_seconds = fixed_time.delta_secs().max(0.0);
    transport_vehicles.par_iter_mut().for_each(
        |(
            vehicle_entity,
            transport_vehicle,
            mut route_position,
            mut station_destination,
            mut vehicle_transform,
            mut vehicle_velocity,
            circuit_member,
            direct_locomotion,
            mut arrival,
        )| {
            let Ok((transport_circuit, circuit_running, circuit_direction)) =
                transport_circuits.get(circuit_member.0)
            else {
                return;
            };
            if transport_circuit.closed || circuit_running.is_some_and(|running| !running.0) {
                vehicle_velocity.set_if_neq(Velocity(Vec3::ZERO));
                return;
            }
            let Ok((_, track_segment, track_circuit_member)) =
                track_segments.get(route_position.segment)
            else {
                return;
            };
            if track_circuit_member.0 != circuit_member.0 {
                return;
            }
            let Ok(distances) = track_travel_distances.get(route_position.segment) else {
                return;
            };
            let authored_reverse_direction =
                matches!(circuit_direction, Some(CircuitDirection::Reverse));
            let requested_throttle = direct_locomotion.map_or(1.0, |direct_locomotion| {
                direct_locomotion
                    .local_axes
                    .is_finite()
                    .then_some(direct_locomotion.local_axes.y.clamp(-1.0, 1.0))
                    .unwrap_or(0.0)
            });
            if requested_throttle == 0.0 {
                vehicle_velocity.set_if_neq(Velocity(Vec3::ZERO));
                return;
            }
            // User-driven transport remains constrained to its canonical track.
            // Positive throttle follows the circuit-facing direction and negative
            // throttle retreats along the same edge.
            let moving_in_reverse =
                authored_reverse_direction ^ requested_throttle.is_sign_negative();
            let requested_travel_distance = transport_vehicle.maximum_speed_metres_per_second
                * requested_throttle.abs()
                * fixed_delta_seconds;
            let mut candidate_route_position = *route_position;
            let reached_segment_endpoint = step_transport_route_position(
                &mut candidate_route_position,
                track_segment.length,
                requested_travel_distance,
                moving_in_reverse,
            );
            let (candidate_translation, mut forward_displacement) =
                distances.translation_and_tangent(track_segment, candidate_route_position.distance);
            let consumed_displacement = candidate_translation - vehicle_transform.translation;
            let movement_is_obstructed =
                Dir3::new(consumed_displacement).is_ok_and(|movement_direction| {
                    spatial_query
                        .cast_ray_predicate(
                            vehicle_transform.translation,
                            movement_direction,
                            consumed_displacement.length(),
                            false,
                            &SpatialQueryFilter::DEFAULT,
                            &|hit_entity| {
                                let owners = [vehicle_entity, track_segment.from, track_segment.to];
                                !owners.contains(&hit_entity)
                                    && !entity_parents
                                        .iter_ancestors::<ChildOf>(hit_entity)
                                        .any(|ancestor| owners.contains(&ancestor))
                            },
                        )
                        .is_some()
                });
            if movement_is_obstructed {
                vehicle_velocity.set_if_neq(Velocity(Vec3::ZERO));
                return;
            }
            *route_position = candidate_route_position;
            vehicle_transform.translation = candidate_translation;
            vehicle_velocity.set_if_neq(Velocity(if fixed_delta_seconds > 0.0 {
                consumed_displacement / fixed_delta_seconds
            } else {
                Vec3::ZERO
            }));

            // Face along the track tangent, reversing it when travelling backwards.
            if moving_in_reverse {
                forward_displacement = -forward_displacement;
            }
            if let Ok(forward_direction) = Dir3::new(forward_displacement) {
                vehicle_transform.rotation = Quat::from_rotation_arc(Vec3::Z, *forward_direction);
            }
            if !reached_segment_endpoint {
                return;
            }

            let reached_station_entity = if moving_in_reverse {
                track_segment.from
            } else {
                track_segment.to
            };
            if transport_stations.contains(reached_station_entity) {
                arrival.0 = Some(reached_station_entity);
            }
            if let Some((next_segment_entity, next_track_segment)) = track_segments
                .iter()
                .filter(|(_, _, track_circuit_member)| track_circuit_member.0 == circuit_member.0)
                .find_map(|(track_segment_entity, candidate_track_segment, _)| {
                    if moving_in_reverse {
                        (candidate_track_segment.to == track_segment.from)
                            .then_some((track_segment_entity, candidate_track_segment))
                    } else {
                        (candidate_track_segment.from == track_segment.to)
                            .then_some((track_segment_entity, candidate_track_segment))
                    }
                })
            {
                route_position.segment = next_segment_entity;
                route_position.distance = if moving_in_reverse {
                    next_track_segment.length
                } else {
                    0.0
                };
                let candidate_destination = if moving_in_reverse {
                    next_track_segment.from
                } else {
                    next_track_segment.to
                };
                if transport_stations.contains(candidate_destination) {
                    station_destination.0 = candidate_destination;
                }
            }
        },
    );
}

pub(super) fn publish_transport_vehicle_arrivals(
    mut vehicles: Query<
        (Entity, &mut TransportVehicleArrival),
        (
            Changed<TransportVehicleArrival>,
            With<TransportVehicle>,
            With<RoutePosition>,
            With<StationDestination>,
            With<Transform>,
            With<Velocity>,
            With<CircuitMember>,
        ),
    >,
    mut arrivals: MessageWriter<Arrived>,
) {
    for (entity, mut arrival) in &mut vehicles {
        if let Some(station) = arrival.0 {
            arrival.0 = None;
            arrivals.write(Arrived {
                entity,
                request_id: 0,
                target: Some(station),
            });
        }
    }
}

/// Drives Bevy-owned playback from the distance that traversal actually
/// consumed. Circuit and collision stops pause locomotion playback.
pub(super) fn project_consumed_transport_vehicle_movement_into_animation_speed(
    transport_vehicles: Query<(&TransportVehicle, &Velocity)>,
    mut transport_animators: Query<(
        &openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
        &mut AnimationPlaybackController,
    )>,
) {
    for (animation_owner, mut animation_playback_controller) in &mut transport_animators {
        let Ok((transport_vehicle, vehicle_velocity)) =
            transport_vehicles.get(animation_owner.gameplay_entity)
        else {
            continue;
        };
        let speed = if transport_vehicle.maximum_speed_metres_per_second > 0.0 {
            (1_000.0 * vehicle_velocity.0.length()
                / transport_vehicle.maximum_speed_metres_per_second)
                .clamp(0.0, f32::from(i16::MAX)) as i16
        } else {
            0
        };
        if animation_playback_controller.playback_speed_permille != speed {
            animation_playback_controller.playback_speed_permille = speed;
        }
    }
}
