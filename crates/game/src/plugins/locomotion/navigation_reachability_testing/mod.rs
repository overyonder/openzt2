use crate::plugins::animal_lifecycle::types::AnimalVariant;
use bevy::prelude::*;

use super::{
    animal_navigation_policy::AnimalNavigationPolicy,
    locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, LocomotionCapacity, LocomotionMode, NavAgent,
        NavigationFailure, NavigationOverlay, ReachabilityTested, Route, RouteScratch,
        TestReachability,
    },
    pathfinding::plan_route,
};

pub(super) fn test_requested_navigation_destination_reachability(
    mut reachability_test_requests: MessageReader<TestReachability>,
    mut reachability_test_results: MessageWriter<ReachabilityTested>,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    navigation_edge_overlay: Res<NavigationOverlay>,
    locomotion_capacity: Option<Res<LocomotionCapacity>>,
    navigation_agents: Query<(
        &NavAgent,
        &LocomotionMode,
        &GlobalTransform,
        Option<&AnimalVariant>,
    )>,
    animal_policy: AnimalNavigationPolicy,
    mut reachability_test_route: Local<Option<Route>>,
    mut pathfinding_scratch: Local<RouteScratch>,
) {
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
    let terrain_derived_navigation_graph =
        active_navigation_graph.terrain_derived_navigation_graph();
    pathfinding_scratch.ensure_capacity(
        terrain_derived_navigation_graph.navigation_node_count(),
        locomotion_capacity.max_route_points,
    );
    let reachability_test_route = reachability_test_route
        .get_or_insert_with(|| Route::with_capacity(locomotion_capacity.max_route_points));
    for reachability_test_request in reachability_test_requests.read() {
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
                    let navigation_agent = animal_policy
                        .agent_with_authored_radius(navigation_agent, animal_variant)?;
                    plan_route(
                        terrain_derived_navigation_graph,
                        &navigation_edge_overlay,
                        &navigation_agent,
                        *locomotion_mode,
                        navigation_agent_transform.translation(),
                        reachability_test_request.destination,
                        reachability_test_route,
                        &mut pathfinding_scratch,
                    )
                },
            );
        reachability_test_results.write(ReachabilityTested {
            entity: reachability_test_request.entity,
            result: reachability_result,
        });
    }
}
