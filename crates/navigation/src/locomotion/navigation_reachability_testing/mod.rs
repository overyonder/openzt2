use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater;

use crate::locomotion::animal_navigation_policy::AnimalNavigationPolicy;
use crate::locomotion::fence_navigation_crossing_types::FenceNavigationCrossingQueries;
use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use crate::locomotion::pathfinding::plan_route;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::ReachabilityTested;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::RouteScratch;
use openzt2_zoo_state::locomotion::locomotion_types::TestReachability;

pub(super) fn test_requested_navigation_destination_reachability(
    mut reachability_test_requests: MessageReader<TestReachability>,
    mut reachability_test_results: MessageWriter<ReachabilityTested>,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    water: Query<&TankNavigationWater, With<WorldRoot>>,
    fence_crossing_index: FenceNavigationCrossingQueries,
    locomotion_capacity: Option<Res<LocomotionCapacity>>,
    navigation_agents: Query<(
        &NavAgent,
        &LocomotionMode,
        &GlobalTransform,
        Option<&AnimalVariant>,
    )>,
    animal_policy: AnimalNavigationPolicy,
    fence_policy: FenceNavigationPolicy,
) {
    let tank_water = water.single().ok();
    let Some(locomotion_capacity) =
        locomotion_capacity.filter(|locomotion_capacity| locomotion_capacity.is_valid())
    else {
        for reachability_test_request in reachability_test_requests.read() {
            reachability_test_results.write(ReachabilityTested {
                entity: reachability_test_request.entity,
                result: Err(NavigationFailure::NoRoute),
            });
        }
        return;
    };
    let Some(active_navigation_graph) = active_navigation_graph else {
        for reachability_test_request in reachability_test_requests.read() {
            reachability_test_results.write(ReachabilityTested {
                entity: reachability_test_request.entity,
                result: Err(NavigationFailure::NoRoute),
            });
        }
        return;
    };
    let locomotion_capacity = *locomotion_capacity;
    let terrain_derived_navigation_graph =
        active_navigation_graph.terrain_derived_navigation_graph();
    let requests: Vec<_> = reachability_test_requests.read().collect();
    if requests.is_empty() {
        return;
    }
    let worker_count = requests.len().min(4);
    let chunk_size = requests.len().div_ceil(worker_count);
    let mut results: Vec<_> = bevy::tasks::ComputeTaskPool::get().scope(|scope| {
        for (chunk_index, chunk) in requests.chunks(chunk_size).enumerate() {
            let navigation_agents = &navigation_agents;
            let animal_policy = &animal_policy;
            let fence_policy = &fence_policy;
            let fence_crossing_index = &fence_crossing_index;
            scope.spawn(async move {
                let mut pathfinding_scratch = RouteScratch::default();
                pathfinding_scratch.ensure_capacity(
                    terrain_derived_navigation_graph.navigation_node_count(),
                    locomotion_capacity.max_route_points,
                );
                let mut reachability_test_route =
                    Route::with_capacity(locomotion_capacity.max_route_points);
                let results = chunk
                    .iter()
                    .map(|reachability_test_request| {
                        reachability_test_route.clear();
                        let reachability_result = navigation_agents
                            .get(reachability_test_request.entity)
                            .map_err(|_| NavigationFailure::NoRoute)
                            .and_then(
                                |(
                                    navigation_agent,
                                    locomotion_mode,
                                    navigation_agent_transform,
                                    animal_variant,
                                )| {
                                    let (navigation_agent, maximum_ground_slope_radians) =
                                        animal_policy.agent_with_authored_collision_limits(
                                            navigation_agent,
                                            animal_variant,
                                        )?;
                                    plan_route(
                                        terrain_derived_navigation_graph,
                                        tank_water,
                                        |from, to| {
                                            fence_crossing_index.crossing_fences(from, to).any(
                                                |fence| {
                                                    fence_policy.fence_blocks_actor(
                                                        fence,
                                                        navigation_agent.capabilities,
                                                        fence_policy.actor_types.actor_types(
                                                            reachability_test_request.entity,
                                                        ),
                                                    )
                                                },
                                            )
                                        },
                                        |from, to| {
                                            fence_policy.world_segment_blocks_actor(
                                                from,
                                                to,
                                                navigation_agent.capabilities,
                                                fence_policy
                                                    .actor_types
                                                    .actor_types(reachability_test_request.entity),
                                            )
                                        },
                                        &navigation_agent,
                                        *locomotion_mode,
                                        maximum_ground_slope_radians,
                                        navigation_agent_transform.translation(),
                                        reachability_test_request.destination,
                                        &mut reachability_test_route,
                                        &mut pathfinding_scratch,
                                        crate::locomotion::pathfinding::SHORT_PATH_ITERATIONS,
                                    )
                                },
                            );
                        ReachabilityTested {
                            entity: reachability_test_request.entity,
                            result: reachability_result,
                        }
                    })
                    .collect::<Vec<_>>();
                (chunk_index, results)
            });
        }
    });
    results.sort_unstable_by_key(|(index, _)| *index);
    for (_, results) in results {
        let _ = reachability_test_results.write_batch(results);
    }
}
