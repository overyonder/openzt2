//! Direct steering at a live entity: a one-point route on its current position.

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

use crate::locomotion::animal_navigation_policy::AnimalNavigationPolicy;
use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use crate::locomotion::pathfinding::straight_line_sweep_admits_agent;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::EntityPursuit;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::PursueEntity;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn accept_entity_pursuit_requests(
    mut commands: Commands,
    mut pursuit_requests: MessageReader<PursueEntity>,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    pursuers: Query<(
        &NavAgent,
        Option<&LocomotionMode>,
        &GlobalTransform,
        Option<&AnimalVariant>,
        Option<&EntityPursuit>,
        Option<&Destination>,
    )>,
    targets: Query<(&GlobalTransform, Option<&LocomotionMode>)>,
    animal_policy: AnimalNavigationPolicy,
    fence_policy: FenceNavigationPolicy,
    mut route_requests: MessageWriter<NavigateTo>,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    for request in pursuit_requests.read() {
        let Ok((target_transform, target_mode)) = targets.get(request.target) else {
            navigation_failures.write(NavigationFailed {
                entity: request.entity,
                request_id: request.request_id,
                reason: NavigationFailure::TargetGone,
            });
            continue;
        };
        let target = target_transform.translation();
        if !request.arrival_radius_m.is_finite() || request.arrival_radius_m < 0.0 {
            navigation_failures.write(NavigationFailed {
                entity: request.entity,
                request_id: request.request_id,
                reason: NavigationFailure::OutsideWorld,
            });
            continue;
        }
        let pursuer = pursuers.get(request.entity).ok();
        let continuing = pursuer
            .and_then(|(.., current, _)| current)
            .is_some_and(|current| current.request_id == request.request_id);
        let line_is_clear = pursuer.is_some_and(|(agent, mode, transform, variant, ..)| {
            let (Some(graph), Ok((agent, maximum_ground_slope_radians))) = (
                active_navigation_graph.as_deref(),
                animal_policy.agent_with_authored_collision_limits(agent, variant),
            ) else {
                return false;
            };
            let from = transform.translation();
            straight_line_sweep_admits_agent(
                graph.terrain_derived_navigation_graph(),
                &agent,
                mode.copied().unwrap_or_default(),
                maximum_ground_slope_radians,
                from,
                target,
                target_mode.copied(),
            ) && !fence_policy.world_segment_blocks_actor(
                from,
                target,
                agent.capabilities,
                fence_policy.actor_types.actor_types(request.entity),
            ) && !fence_policy.gate_blocks_movement(
                request.entity,
                agent.capabilities,
                from,
                target,
            )
        });
        if line_is_clear {
            // A clear re-test leaves the live steering as it is.
            if !continuing {
                commands
                    .entity(request.entity)
                    .insert((
                        Destination {
                            request_id: request.request_id,
                            world: target,
                            arrival_radius_m: request.arrival_radius_m,
                        },
                        EntityPursuit {
                            request_id: request.request_id,
                            target: request.target,
                        },
                    ))
                    .remove::<Docking>();
            }
            continue;
        }
        if continuing {
            commands.entity(request.entity).remove::<EntityPursuit>();
        }
        navigation_failures.write(NavigationFailed {
            entity: request.entity,
            request_id: request.request_id,
            reason: NavigationFailure::NoRoute,
        });
        let refused_route = request.refused_route;
        let route_is_current = pursuer
            .and_then(|(.., destination)| destination)
            .is_some_and(|destination| destination.request_id == refused_route.request_id);
        if !route_is_current {
            route_requests.write(NavigateTo {
                entity: request.entity,
                request_id: refused_route.request_id,
                destination: target,
                arrival_radius_m: refused_route.arrival_radius_m,
            });
        }
    }
}

/// Runs before route planning, which never plans for a pursuing agent. The
/// pursuer aims one second ahead of the target along the target's current
/// motion. Only a swimmer keeps the target's height; every other pursuer
/// steers in its own horizontal plane.
#[allow(clippy::type_complexity)]
pub(super) fn steer_entity_pursuits_toward_live_targets(
    mut commands: Commands,
    mut pursuers: Query<(
        Entity,
        &EntityPursuit,
        &GlobalTransform,
        Option<&LocomotionMode>,
        Option<&mut Destination>,
        &mut Route,
    )>,
    targets: Query<(&GlobalTransform, Option<&Velocity>, Option<&LinearVelocity>)>,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    for (pursuer, pursuit, transform, mode, destination, mut route) in &mut pursuers {
        // Arrival, failure or a replacement request ended this pursuit.
        let Some(mut destination) =
            destination.filter(|destination| destination.request_id == pursuit.request_id)
        else {
            commands.entity(pursuer).remove::<EntityPursuit>();
            continue;
        };
        let Ok((target, velocity, linear_velocity)) = targets.get(pursuit.target) else {
            route.clear();
            commands
                .entity(pursuer)
                .remove::<(EntityPursuit, Destination)>();
            navigation_failures.write(NavigationFailed {
                entity: pursuer,
                request_id: pursuit.request_id,
                reason: NavigationFailure::TargetGone,
            });
            continue;
        };
        let motion = linear_velocity
            .map(|velocity| velocity.0)
            .or_else(|| velocity.map(|velocity| velocity.0))
            .filter(|motion| motion.is_finite())
            .unwrap_or(Vec3::ZERO);
        let mut position = target.translation() + motion;
        if mode.copied().unwrap_or_default() != LocomotionMode::Swim {
            position.y = transform.translation().y;
        }
        route.clear();
        route.points.push(position);
        // Moving the endpoint must not look like a new request to the planner.
        destination.bypass_change_detection().world = position;
    }
}
