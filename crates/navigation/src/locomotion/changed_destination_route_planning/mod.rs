use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

use crate::locomotion::animal_navigation_policy::AnimalNavigationPolicy;
use crate::locomotion::fence_navigation_crossing_types::FenceNavigationCrossingIndex;
use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use crate::locomotion::pathfinding::{
    ITERATIONS_PER_SLICE, LONG_PATH_ITERATIONS, RouteSearch, advance_route,
};
use std::collections::{HashSet, VecDeque};

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::EntityPursuit;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::RouteScratch;

#[derive(Default)]
pub(super) struct RouteQueue {
    waiting: VecDeque<Entity>,
    queued: HashSet<Entity>,
    search: RouteSearch,
}

#[allow(clippy::type_complexity)]
pub(super) fn plan_routes_for_changed_navigation_destinations(
    mut commands: Commands,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    fence_crossing_index: Res<FenceNavigationCrossingIndex>,
    locomotion_capacity: Option<Res<LocomotionCapacity>>,
    mut route_planning_scratch: Local<RouteScratch>,
    animal_policy: AnimalNavigationPolicy,
    fence_policy: FenceNavigationPolicy,
    changed: Query<Entity, Changed<Destination>>,
    mut queue: Local<RouteQueue>,
    mut agents: Query<
        (
            Entity,
            &NavAgent,
            Option<&AnimalVariant>,
            &GlobalTransform,
            &Destination,
            Option<&LocomotionMode>,
            &mut Route,
            Option<&openzt2_zoo_state::locomotion::locomotion_types::RestoredNavigationRoute>,
        ),
        (
            Without<DirectLocomotion>,
            Without<EntityPursuit>,
            Without<openzt2_zoo_state::behavior_task_execution_types::BehaviorEvasionSteering>,
        ),
    >,
    mut navigation_failures: MessageWriter<NavigationFailed>,
    terrain_index: Res<openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>,
    terrain_assets: Res<
        Assets<
            openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
        >,
    >,
    terrain_chunks: Query<(
        &openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk,
        Option<&openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples>,
    )>,
) {
    // Invalidate a suspended search even when its next slice cannot run yet.
    if fence_crossing_index.is_changed()
        || active_navigation_graph
            .as_ref()
            .is_some_and(|graph| graph.is_changed())
    {
        queue.search = RouteSearch::default();
    }
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
    for entity in &changed {
        let Ok((_, _, _, _, destination, _, mut route, restored)) = agents.get_mut(entity) else {
            continue;
        };
        if queue.waiting.front() == Some(&entity) {
            queue.search = RouteSearch::default();
        }
        if let Some(restored) = restored {
            commands.entity(entity).remove::<openzt2_zoo_state::locomotion::locomotion_types::RestoredNavigationRoute>();
            if restored.request_id == destination.request_id && !route.points.is_empty() {
                if queue.queued.remove(&entity) {
                    queue.waiting.retain(|&queued| queued != entity);
                }
                continue;
            }
        }
        route.clear();
        if queue.queued.insert(entity) {
            queue.waiting.push_back(entity);
        }
    }
    while let Some(entity) = queue.waiting.front().copied() {
        let Ok((
            navigation_agent_entity,
            navigation_agent,
            animal_variant,
            global_transform,
            destination,
            locomotion_mode,
            mut planned_route,
            _,
        )) = agents.get_mut(entity)
        else {
            queue.waiting.pop_front();
            queue.queued.remove(&entity);
            queue.search = RouteSearch::default();
            continue;
        };
        let route_planning_result = animal_policy
            .agent_with_authored_collision_limits(navigation_agent, animal_variant)
            .and_then(|(navigation_agent, maximum_ground_slope_radians)| {
                advance_route(
                    terrain_navigation_graph,
                    |from, to| {
                        fence_crossing_index
                            .crossing_fences(from, to)
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
                    &mut queue.search,
                    ITERATIONS_PER_SLICE,
                    LONG_PATH_ITERATIONS,
                )
            });
        if matches!(route_planning_result, Ok(false)) {
            break;
        }
        queue.waiting.pop_front();
        queue.queued.remove(&entity);
        queue.search = RouteSearch::default();
        if route_planning_result.is_ok()
            && navigation_agent
                .capabilities
                .contains(openzt2_zoo_state::locomotion::locomotion_types::NavFlags::AMPHIBIOUS)
        {
            for point in &mut planned_route.points {
                super::spatial_index_and_docking_operations::fit_to_terrain(
                    LocomotionMode::Swim,
                    point,
                    &terrain_index,
                    &terrain_assets,
                    &terrain_chunks,
                );
            }
        }
        if let Err(navigation_failure_reason) = route_planning_result {
            debug!(agent = ?navigation_agent_entity, reason = ?navigation_failure_reason, start = ?global_transform.translation(), destination = ?destination.world, "route planning failed");
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
        break;
    }
}
