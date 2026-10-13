use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::habitat::habitat_types::Containment;

use crate::locomotion::animal_navigation_policy::AnimalNavigationPolicy;
use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use crate::locomotion::pathfinding::plan_route;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationOverlay;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::RouteScratch;

#[allow(clippy::type_complexity)]
pub(super) fn plan_routes_for_changed_navigation_destinations(
    mut commands: Commands,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    navigation_overlay: Res<NavigationOverlay>,
    locomotion_capacity: Option<Res<LocomotionCapacity>>,
    mut route_planning_scratch: Local<RouteScratch>,
    animal_policy: AnimalNavigationPolicy,
    fence_policy: FenceNavigationPolicy,
    mut navigation_agents_with_changed_destinations: Query<
        (
            Entity,
            &NavAgent,
            Option<&AnimalVariant>,
            &GlobalTransform,
            &Destination,
            Option<&LocomotionMode>,
            Option<&Containment>,
            &mut Route,
        ),
        (Changed<Destination>, Without<DirectLocomotion>),
    >,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    let Some(locomotion_capacity) =
        locomotion_capacity.filter(|locomotion_capacity| locomotion_capacity.is_valid())
    else {
        return;
    };
    let Some(active_navigation_graph) = active_navigation_graph else {
        return;
    };
    let terrain_navigation_graph = active_navigation_graph.terrain_derived_navigation_graph();
    route_planning_scratch.ensure_capacity(
        terrain_navigation_graph.navigation_node_count(),
        locomotion_capacity.max_route_points,
    );
    for (
        navigation_agent_entity,
        navigation_agent,
        animal_variant,
        global_transform,
        destination,
        locomotion_mode,
        containment,
        mut planned_route,
    ) in &mut navigation_agents_with_changed_destinations
    {
        if containment.is_some_and(|containment| !containment.is_contained) {
            planned_route.clear();
            commands
                .entity(navigation_agent_entity)
                .remove::<Destination>();
            navigation_failures.write(NavigationFailed {
                entity: navigation_agent_entity,
                request_id: destination.request_id,
                reason: NavigationFailure::ContainmentBlocked,
            });
            continue;
        }
        let route_planning_result = animal_policy
            .agent_with_authored_collision_limits(navigation_agent, animal_variant)
            .and_then(|(navigation_agent, maximum_ground_slope_radians)| {
                plan_route(
                    terrain_navigation_graph,
                    |from, to| {
                        navigation_overlay
                            .fence_crossings(from, to)
                            .iter()
                            .any(|fence| {
                                fence_policy.fence_blocks_actor(
                                    *fence,
                                    navigation_agent.capabilities,
                                    fence_policy
                                        .actor_types
                                        .actor_types(navigation_agent_entity),
                                )
                            })
                    },
                    |from, to| {
                        fence_policy.world_segment_blocks_actor(
                            from,
                            to,
                            navigation_agent.capabilities,
                            fence_policy
                                .actor_types
                                .actor_types(navigation_agent_entity),
                        )
                    },
                    &navigation_agent,
                    locomotion_mode.copied().unwrap_or_default(),
                    maximum_ground_slope_radians,
                    global_transform.translation(),
                    destination.world,
                    &mut planned_route,
                    &mut route_planning_scratch,
                )
            });
        if let Err(navigation_failure_reason) = route_planning_result {
            // An exhausted search leaves the nodes reachable from the start
            // closed; their count and extent show what encloses the agent.
            let (reachable_nodes, reachable_min_cm, reachable_max_cm) = route_planning_scratch
                .closed
                .iter()
                .enumerate()
                .filter(|(_, closed)| **closed)
                .filter_map(|(node, _)| {
                    terrain_navigation_graph
                        .calculate_navigation_node_world_position_centimetres(node as u32)
                })
                .fold(
                    (0_u32, [i32::MAX; 3], [i32::MIN; 3]),
                    |(count, minimum, maximum), position| {
                        (
                            count + 1,
                            std::array::from_fn(|axis| minimum[axis].min(position[axis])),
                            std::array::from_fn(|axis| maximum[axis].max(position[axis])),
                        )
                    },
                );
            debug!(agent = ?navigation_agent_entity, reason = ?navigation_failure_reason,
                start = ?global_transform.translation(), destination = ?destination.world,
                capabilities = ?navigation_agent.capabilities, reachable_nodes,
                ?reachable_min_cm, ?reachable_max_cm, "route planning failed");
            planned_route.clear();
            commands
                .entity(navigation_agent_entity)
                .remove::<Destination>();
            navigation_failures.write(NavigationFailed {
                entity: navigation_agent_entity,
                request_id: destination.request_id,
                reason: navigation_failure_reason,
            });
        }
    }
}
