use bevy::prelude::*;

use crate::plugins::animal_lifecycle::types::AnimalVariant;
use crate::plugins::habitat::habitat_types::Containment;

use super::{
    animal_navigation_policy::AnimalNavigationPolicy,
    locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Destination, DirectLocomotion, LocomotionCapacity,
        LocomotionMode, NavAgent, NavigationFailed, NavigationFailure, NavigationOverlay, Route,
        RouteScratch,
    },
    pathfinding::plan_route,
};

#[allow(clippy::type_complexity)]
pub(super) fn plan_routes_for_changed_navigation_destinations(
    mut commands: Commands,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    navigation_overlay: Res<NavigationOverlay>,
    locomotion_capacity: Option<Res<LocomotionCapacity>>,
    mut route_planning_scratch: Local<RouteScratch>,
    animal_policy: AnimalNavigationPolicy,
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
            .agent_with_authored_radius(navigation_agent, animal_variant)
            .and_then(|navigation_agent| {
                plan_route(
                    terrain_navigation_graph,
                    &navigation_overlay,
                    &navigation_agent,
                    locomotion_mode.copied().unwrap_or_default(),
                    global_transform.translation(),
                    destination.world,
                    &mut planned_route,
                    &mut route_planning_scratch,
                )
            });
        if let Err(navigation_failure_reason) = route_planning_result {
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
