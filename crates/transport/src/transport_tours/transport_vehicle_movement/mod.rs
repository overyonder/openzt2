use super::ground_track_policy::{GroundTrackPolicy, GroundTrackPolicyAsset};
use super::sky_station_travel::SkyStationTravel;
use avian3d::prelude::{ColliderAabb, SpatialQuery};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::TerrainChunk,
    terrain_collision_rebuilding_types::TerrainChunkCollisionSurface,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackIncidence, TrackRoute};

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
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::StationDestination;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::{RoutePosition, VehicleOnTrack};
use openzt2_zoo_state::transport_tours::transport_vehicle_types::{
    TransportVehicle, TransportVehicleArrival,
};

#[derive(SystemParam)]
pub(super) struct GroundTrackObstruction<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    bounds: Query<
        'w,
        's,
        &'static ColliderAabb,
        (Without<TerrainChunk>, Without<TerrainChunkCollisionSurface>),
    >,
    velocities: Query<'w, 's, &'static Velocity, Without<TransportVehicle>>,
    vehicles: Query<'w, 's, (), With<TransportVehicle>>,
    visibility: Query<'w, 's, &'static InheritedVisibility>,
}

impl GroundTrackObstruction<'_, '_> {
    fn distance_ahead(
        &self,
        from: Vec3,
        to: Vec3,
        excluded: [Entity; 3],
        policy: &GroundTrackPolicy,
    ) -> Option<f32> {
        let span = to.xz() - from.xz();
        let length = span.length();
        if !length.is_finite() || length <= f32::EPSILON {
            return None;
        }
        let heading = span / length;
        let radius = policy.collision_radius;
        let pad = Vec3::splat(radius);
        let mut closest = None::<f32>;
        self.spatial.aabb_intersections_with_aabb_callback(
            ColliderAabb {
                min: from.min(to) - pad,
                max: from.max(to) + pad,
            },
            |entity| {
                let Ok(bounds) = self.bounds.get(entity) else {
                    return true;
                };
                let mut owners =
                    std::iter::once(entity).chain(self.parents.iter_ancestors::<ChildOf>(entity));
                if excluded.contains(&entity)
                    || self.vehicles.contains(entity)
                    || self
                        .parents
                        .iter_ancestors::<ChildOf>(entity)
                        .any(|owner| excluded.contains(&owner) || self.vehicles.contains(owner))
                    || self
                        .visibility
                        .get(entity)
                        .is_ok_and(|visibility| !visibility.get())
                {
                    return true;
                }
                let centre = bounds.min.midpoint(bounds.max).xz();
                let extent = (bounds.max - bounds.min).xz().length() * 0.5;
                let relative = centre - from.xz();
                let along = relative.dot(heading);
                let range = radius + extent;
                let static_intersection =
                    (0.0..=length).contains(&along) && relative.perp_dot(heading).abs() < range;
                let intersection = if static_intersection {
                    Some(along)
                } else {
                    let velocity = owners
                        .find_map(|owner| self.velocities.get(owner).ok())
                        .map_or(Vec2::ZERO, |velocity| velocity.0.xz());
                    let side_speed = velocity.perp_dot(heading);
                    if velocity.length_squared() < policy.minimum_obstruction_speed_squared
                        || side_speed == 0.0
                    {
                        return true;
                    }
                    let arrival = -relative.perp_dot(heading) / side_speed;
                    let intersection = along + velocity.dot(heading) * arrival;
                    (arrival >= 0.0 && (0.0..=length).contains(&intersection))
                        .then_some(intersection)
                };
                if let Some(intersection) = intersection {
                    let distance = (intersection / length * from.distance(to) - extent).max(0.0);
                    closest = Some(closest.map_or(distance, |previous| previous.min(distance)));
                }
                closest != Some(0.0)
            },
        );
        closest
    }
}

/// Carries a cabin between the two cable sockets of a station.
#[derive(Component)]
pub struct SkyStationCrossing {
    pub next_track: Entity,
    pub reverse: bool,
    pub distance: f32,
}

pub(super) fn advance_transport_vehicles_along_canonical_track_segments(
    (obstructions, ground_policy): (GroundTrackObstruction, GroundTrackPolicyAsset),
    commands: bevy::ecs::system::ParallelCommands,
    incidence: TrackIncidence,
    fixed_time: Res<Time<Fixed>>,
    mut transport_vehicles: Query<(
        Entity,
        &TransportVehicle,
        &mut RoutePosition,
        &VehicleOnTrack,
        &mut StationDestination,
        &mut Transform,
        &mut Velocity,
        &CircuitMember,
        Option<&DirectLocomotion>,
        &mut TransportVehicleArrival,
        Option<&mut SkyStationCrossing>,
    )>,
    track_segments: Query<(
        Entity,
        TrackRoute,
        &CircuitMember,
        &TrackTravelDistances,
        &TrackProfile,
    )>,
    transport_stations: Query<Option<&SkyStationTravel>, With<TransportStation>>,
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
            current_track,
            mut station_destination,
            mut vehicle_transform,
            mut vehicle_velocity,
            circuit_member,
            direct_locomotion,
            mut arrival,
            crossing,
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
            let Ok((_, track_segment, track_circuit_member, distances, profile)) =
                track_segments.get(current_track.0)
            else {
                return;
            };
            if track_circuit_member.0 != circuit_member.0 {
                return;
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
            if let Some(mut crossing) = crossing {
                let Ok((_, next_track, next_member, next_distances, _)) =
                    track_segments.get(crossing.next_track)
                else {
                    commands.command_scope(|mut commands| {
                        commands
                            .entity(vehicle_entity)
                            .remove::<SkyStationCrossing>();
                    });
                    return;
                };
                if next_member.0 != circuit_member.0 {
                    commands.command_scope(|mut commands| {
                        commands
                            .entity(vehicle_entity)
                            .remove::<SkyStationCrossing>();
                    });
                    return;
                }
                let next_length = next_distances.travel_length(next_track.geometry);
                let exit_distance = if crossing.reverse { next_length } else { 0.0 };
                let (entry, _) = distances.translation_and_tangent(
                    track_segment.geometry,
                    if crossing.reverse {
                        0.0
                    } else {
                        distances.travel_length(track_segment.geometry)
                    },
                );
                let (exit, _) =
                    next_distances.translation_and_tangent(next_track.geometry, exit_distance);
                let station = if crossing.reverse {
                    track_segment.from()
                } else {
                    track_segment.to()
                };
                let Some((path, path_reverse)) = transport_stations
                    .get(station)
                    .ok()
                    .flatten()
                    .and_then(|travel| travel.between(entry, exit))
                else {
                    vehicle_velocity.set_if_neq(Velocity(Vec3::ZERO));
                    return;
                };
                let span_length = path.length();
                let reversing = moving_in_reverse != crossing.reverse;
                let before = crossing.distance;
                let step = if reversing {
                    -requested_travel_distance
                } else {
                    requested_travel_distance
                };
                let mut after = (before + step).clamp(0.0, span_length);
                if let Some(stop) = path.stop(path_reverse ^ reversing) {
                    let stop = if path_reverse {
                        span_length - stop
                    } else {
                        stop
                    };
                    if (!reversing && before < stop && stop <= after)
                        || (reversing && after <= stop && stop < before)
                    {
                        after = stop;
                        arrival.0 = Some(station);
                    }
                }
                if before != after {
                    crossing.distance = after;
                }
                let (next, tangent) = path.pose(if path_reverse {
                    span_length - after
                } else {
                    after
                });
                let heading = tangent.normalize_or_zero();
                let displacement = next - vehicle_transform.translation;
                vehicle_transform.translation = next;
                let forward = if reversing ^ path_reverse {
                    -heading
                } else {
                    heading
                };
                vehicle_transform.rotation = Quat::from_rotation_arc(Vec3::Z, forward);
                vehicle_velocity.set_if_neq(Velocity(if fixed_delta_seconds > 0.0 {
                    displacement / fixed_delta_seconds
                } else {
                    Vec3::ZERO
                }));
                if (!reversing && crossing.distance == span_length)
                    || (reversing && crossing.distance == 0.0)
                {
                    commands.command_scope(|mut commands| {
                        let mut vehicle = commands.entity(vehicle_entity);
                        vehicle.remove::<SkyStationCrossing>();
                        if !reversing {
                            vehicle.insert(VehicleOnTrack(crossing.next_track));
                        }
                    });
                    if !reversing {
                        route_position.distance = exit_distance;
                        let destination = if crossing.reverse {
                            next_track.from()
                        } else {
                            next_track.to()
                        };
                        if transport_stations.contains(destination) {
                            station_destination.0 = destination;
                        }
                    }
                }
                return;
            }
            let mut candidate_route_position = *route_position;
            let track_length = distances.travel_length(track_segment.geometry);
            let travel = if profile.kind == TransportationTrackKind::Ground {
                ground_policy
                    .get()
                    .and_then(|policy| {
                        let lookahead = requested_travel_distance + policy.collision_radius;
                        let sign = if moving_in_reverse { -1.0 } else { 1.0 };
                        let probe =
                            (route_position.distance + sign * lookahead).clamp(0.0, track_length);
                        distances
                            .ground_spans_between(
                                track_segment.geometry,
                                route_position.distance,
                                probe,
                            )
                            .filter_map(|(offset, [from, to])| {
                                obstructions
                                    .distance_ahead(
                                        from,
                                        to,
                                        [vehicle_entity, track_segment.from(), track_segment.to()],
                                        policy,
                                    )
                                    .map(|distance| offset + distance)
                            })
                            .min_by(f32::total_cmp)
                    })
                    .map_or(requested_travel_distance, |obstruction| {
                        requested_travel_distance.min(obstruction)
                    })
            } else {
                requested_travel_distance
            };
            if travel <= f32::EPSILON {
                vehicle_velocity.set_if_neq(Velocity(Vec3::ZERO));
                return;
            }
            let reached_segment_endpoint = step_transport_route_position(
                &mut candidate_route_position,
                track_length,
                travel,
                moving_in_reverse,
            );
            let (candidate_translation, mut forward_displacement) = distances
                .translation_and_tangent(track_segment.geometry, candidate_route_position.distance);
            let consumed_displacement = candidate_translation - vehicle_transform.translation;
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
                track_segment.from()
            } else {
                track_segment.to()
            };
            let reached_station = transport_stations.contains(reached_station_entity);
            let next = if moving_in_reverse {
                incidence
                    .incoming(reached_station_entity)
                    .find_map(|entity| {
                        track_segments
                            .get(entity)
                            .ok()
                            .filter(|(_, _, circuit, _, _)| circuit.0 == circuit_member.0)
                    })
            } else {
                incidence
                    .outgoing(reached_station_entity)
                    .find_map(|entity| {
                        track_segments
                            .get(entity)
                            .ok()
                            .filter(|(_, _, circuit, _, _)| circuit.0 == circuit_member.0)
                    })
            };
            if reached_station && distances.sky_cable().is_none() {
                arrival.0 = Some(reached_station_entity);
            }
            if let Some((next_segment_entity, next_track_segment, _, next_distances, _)) = next {
                if distances.sky_cable().is_some()
                    && next_distances.sky_cable().is_some()
                    && reached_station
                {
                    commands.command_scope(|mut commands| {
                        commands.entity(vehicle_entity).insert(SkyStationCrossing {
                            next_track: next_segment_entity,
                            reverse: moving_in_reverse,
                            distance: 0.0,
                        });
                    });
                    return;
                }
                commands.command_scope(|mut commands| {
                    commands
                        .entity(vehicle_entity)
                        .insert(VehicleOnTrack(next_segment_entity));
                });
                route_position.distance = if moving_in_reverse {
                    next_distances.travel_length(next_track_segment.geometry)
                } else {
                    0.0
                };
                let candidate_destination = if moving_in_reverse {
                    next_track_segment.from()
                } else {
                    next_track_segment.to()
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
