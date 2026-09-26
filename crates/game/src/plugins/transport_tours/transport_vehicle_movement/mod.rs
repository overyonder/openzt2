use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use crate::plugins::{
    animation_playback::animation_playback_controller_types::AnimationPlaybackController,
    locomotion::locomotion_types::{Arrived, DirectLocomotion, Velocity},
};

use super::{
    transport_circuit_types::{CircuitDirection, CircuitMember, CircuitRunning, TransportCircuit},
    transport_fact_validation::transport_track_segment_is_valid,
    transport_hierarchy_queries::entity_is_descendant_of_transport_owner,
    transport_route_position_calculations::{
        advance_route_position_toward_segment_end,
        calculate_forward_displacement_along_transport_track_segment,
        calculate_translation_along_transport_track_segment,
        retreat_route_position_toward_segment_start,
    },
    transport_topology_types::{TrackSegment, TransportStation},
    transport_vehicle_types::{RoutePosition, StationDestination, TransportVehicle},
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
    )>,
    track_segments: Query<(Entity, &TrackSegment, &CircuitMember)>,
    transport_stations: Query<(), With<TransportStation>>,
    entity_parents: Query<&ChildOf>,
    transport_circuits: Query<(
        &TransportCircuit,
        Option<&CircuitRunning>,
        Option<&CircuitDirection>,
    )>,
    mut vehicle_arrivals: MessageWriter<Arrived>,
) {
    let fixed_delta_seconds = fixed_time.delta_secs().max(0.0);
    for (
        vehicle_entity,
        transport_vehicle,
        mut route_position,
        mut station_destination,
        mut vehicle_transform,
        mut vehicle_velocity,
        circuit_member,
        direct_locomotion,
    ) in &mut transport_vehicles
    {
        let Ok((transport_circuit, circuit_running, circuit_direction)) =
            transport_circuits.get(circuit_member.0)
        else {
            continue;
        };
        if transport_circuit.closed || circuit_running.is_some_and(|running| !running.0) {
            vehicle_velocity.0 = Vec3::ZERO;
            continue;
        }
        let Ok((_, track_segment, track_circuit_member)) =
            track_segments.get(route_position.segment)
        else {
            continue;
        };
        if track_circuit_member.0 != circuit_member.0
            || !transport_track_segment_is_valid(track_segment)
        {
            continue;
        }
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
            vehicle_velocity.0 = Vec3::ZERO;
            continue;
        }
        // User-driven transport remains constrained to its canonical track.
        // Positive throttle follows the circuit-facing direction and negative
        // throttle retreats along the same edge.
        let moving_in_reverse = authored_reverse_direction ^ requested_throttle.is_sign_negative();
        let requested_travel_distance = transport_vehicle.maximum_speed_metres_per_second
            * requested_throttle.abs()
            * fixed_delta_seconds;
        let mut candidate_route_position = *route_position;
        let reached_segment_endpoint = if moving_in_reverse {
            retreat_route_position_toward_segment_start(
                &mut candidate_route_position,
                track_segment,
                requested_travel_distance,
            )
        } else {
            advance_route_position_toward_segment_end(
                &mut candidate_route_position,
                track_segment,
                requested_travel_distance,
            )
        };
        let candidate_translation = calculate_translation_along_transport_track_segment(
            track_segment,
            candidate_route_position.distance,
        );
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
                            !entity_is_descendant_of_transport_owner(
                                hit_entity,
                                vehicle_entity,
                                &entity_parents,
                            ) && !entity_is_descendant_of_transport_owner(
                                hit_entity,
                                track_segment.from,
                                &entity_parents,
                            ) && !entity_is_descendant_of_transport_owner(
                                hit_entity,
                                track_segment.to,
                                &entity_parents,
                            )
                        },
                    )
                    .is_some()
            });
        if movement_is_obstructed {
            vehicle_velocity.0 = Vec3::ZERO;
            continue;
        }
        *route_position = candidate_route_position;
        vehicle_transform.translation = candidate_translation;
        vehicle_velocity.0 = if fixed_delta_seconds > 0.0 {
            consumed_displacement / fixed_delta_seconds
        } else {
            Vec3::ZERO
        };

        // Face along the track tangent, reversing it when travelling backwards.
        let mut forward_displacement = calculate_forward_displacement_along_transport_track_segment(
            track_segment,
            candidate_route_position.distance,
        );
        if moving_in_reverse {
            forward_displacement = -forward_displacement;
        }
        if let Ok(forward_direction) = Dir3::new(forward_displacement) {
            vehicle_transform.rotation = Quat::from_rotation_arc(Vec3::Z, *forward_direction);
        }
        if !reached_segment_endpoint {
            continue;
        }

        let reached_station_entity = if moving_in_reverse {
            track_segment.from
        } else {
            track_segment.to
        };
        if transport_stations.contains(reached_station_entity) {
            vehicle_arrivals.write(Arrived {
                entity: vehicle_entity,
                request_id: 0,
                target: Some(reached_station_entity),
            });
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
    }
}

/// Drives Bevy-owned playback from the distance that traversal actually
/// consumed. Circuit and collision stops pause locomotion playback.
pub(super) fn project_consumed_transport_vehicle_movement_into_animation_speed(
    transport_vehicles: Query<(&TransportVehicle, &Velocity)>,
    mut transport_animators: Query<(
        &crate::plugins::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
        &mut AnimationPlaybackController,
    )>,
) {
    for (animation_owner, mut animation_playback_controller) in &mut transport_animators {
        let Ok((transport_vehicle, vehicle_velocity)) =
            transport_vehicles.get(animation_owner.gameplay_entity)
        else {
            continue;
        };
        animation_playback_controller.playback_speed_permille =
            if transport_vehicle.maximum_speed_metres_per_second > 0.0 {
                (1_000.0 * vehicle_velocity.0.length()
                    / transport_vehicle.maximum_speed_metres_per_second)
                    .clamp(0.0, f32::from(i16::MAX)) as i16
            } else {
                0
            };
    }
}
