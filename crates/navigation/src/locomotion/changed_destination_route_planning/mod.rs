use bevy::{prelude::*, utils::Parallel};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant, aquatic::tank_navigation_water::TankNavigationWater,
};

use super::{
    animal_navigation_policy::AnimalNavigationPolicy,
    fence_navigation_crossing_types::FenceNavigationCrossingQueries,
    fence_navigation_policy::FenceNavigationPolicy,
    pathfinding::{advance_route, RouteSearch, ITERATIONS_PER_SLICE, LONG_PATH_ITERATIONS},
};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::{
    behavior_task_execution_types::BehaviorEvasionSteering,
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Destination, DirectLocomotion, EntityPursuit,
        EntranceNavigationConnection, LocomotionCapacity, LocomotionMode, NavAgent, NavFlags,
        NavigationFailed, NavigationFailure, RestoredNavigationRoute, Route, RouteScratch,
        SpatialGrid,
    },
    terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
};

const MAX_ACTIVE_ROUTE_SEARCHES: usize = 4;

#[derive(Component, Default)]
pub(super) struct PendingRouteSearch(Option<ActiveRouteSearch>);

#[derive(Default)]
struct ActiveRouteSearch {
    search: RouteSearch,
    scratch: RouteScratch,
}

type PlannedMovement = (
    Without<DirectLocomotion>,
    Without<EntityPursuit>,
    Without<BehaviorEvasionSteering>,
);

#[allow(clippy::type_complexity)]
pub(super) fn prepare_changed_navigation_destinations(
    mut commands: Commands,
    mut agents: Query<
        (
            Entity,
            &Destination,
            &mut Route,
            Option<&RestoredNavigationRoute>,
            Option<&mut PendingRouteSearch>,
        ),
        (Changed<Destination>, With<NavAgent>, PlannedMovement),
    >,
    stale_connections: Query<Entity, (With<EntranceNavigationConnection>, Without<Destination>)>,
    cancelled: Query<
        Entity,
        (
            With<PendingRouteSearch>,
            Or<(
                Without<Destination>,
                Without<NavAgent>,
                With<DirectLocomotion>,
                With<EntityPursuit>,
                With<BehaviorEvasionSteering>,
            )>,
        ),
    >,
) {
    for entity in &stale_connections {
        commands
            .entity(entity)
            .remove::<EntranceNavigationConnection>();
    }
    for entity in &cancelled {
        commands.entity(entity).remove::<PendingRouteSearch>();
    }
    for (entity, destination, mut route, restored, pending) in &mut agents {
        if let Some(restored) = restored {
            commands.entity(entity).remove::<RestoredNavigationRoute>();
            if restored.request_id == destination.request_id && !route.points.is_empty() {
                if pending.is_some() {
                    commands.entity(entity).remove::<PendingRouteSearch>();
                }
                continue;
            }
        }
        route.clear();
        if let Some(mut pending) = pending {
            if let Some(active) = &mut pending.0 {
                active.search = RouteSearch::default();
            }
        } else {
            commands
                .entity(entity)
                .insert(PendingRouteSearch::default());
        }
    }
}

pub(super) fn admit_pending_route_searches(
    graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    fences: FenceNavigationCrossingQueries,
    capacity: Option<Res<LocomotionCapacity>>,
    mut pending: Query<(Entity, &Destination, &mut PendingRouteSearch)>,
) {
    let (Some(graph), Some(capacity)) = (graph, capacity.filter(|capacity| capacity.is_valid()))
    else {
        return;
    };
    let needs_restart = graph.is_changed() || fences.is_changed();
    let nodes = graph
        .terrain_derived_navigation_graph()
        .navigation_node_count();
    let mut active_count = 0;
    let mut waiting = Vec::new();
    for (entity, destination, mut pending) in &mut pending {
        if let Some(active) = &mut pending.bypass_change_detection().0 {
            if needs_restart {
                active.search = RouteSearch::default();
                active
                    .scratch
                    .ensure_capacity(nodes, capacity.max_route_points);
            }
            active_count += 1;
        } else {
            waiting.push((destination.request_id, entity.to_bits(), entity));
        }
    }
    waiting.sort_unstable();
    for (_, _, entity) in waiting
        .into_iter()
        .take(MAX_ACTIVE_ROUTE_SEARCHES.saturating_sub(active_count))
    {
        if let Ok((_, _, mut pending)) = pending.get_mut(entity) {
            let mut active = ActiveRouteSearch::default();
            active
                .scratch
                .ensure_capacity(nodes, capacity.max_route_points);
            pending.0 = Some(active);
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn advance_active_navigation_searches(
    mut commands: Commands,
    graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    water: Query<&TankNavigationWater, With<WorldRoot>>,
    grid: Res<SpatialGrid>,
    fence_crossing_index: FenceNavigationCrossingQueries,
    animal_policy: AnimalNavigationPolicy,
    fence_policy: FenceNavigationPolicy,
    mut agents: Query<
        (
            Entity,
            &NavAgent,
            Option<&AnimalVariant>,
            &GlobalTransform,
            &Destination,
            Option<&LocomotionMode>,
            &mut Route,
            &mut PendingRouteSearch,
            Option<&EntranceNavigationConnection>,
        ),
        PlannedMovement,
    >,
    mut navigation_failures: MessageWriter<NavigationFailed>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) {
    let Some(graph) = graph else {
        return;
    };
    let navigation = graph.terrain_derived_navigation_graph();
    let tank_water = water.single().ok();
    let mut finished = Parallel::<Vec<(u64, Entity, Option<NavigationFailure>)>>::default();
    agents.par_iter_mut().for_each(
        |(
            entity,
            agent,
            variant,
            transform,
            destination,
            mode,
            mut route,
            mut pending,
            connection,
        )| {
            let Some(active) = &mut pending.0 else {
                return;
            };
            let actor_types = fence_policy.actor_types.actor_types(entity);
            let position = transform.translation();
            let off_map = connection.filter(|connection| {
                connection.request_id == destination.request_id
                    && if connection.inbound {
                        grid.cell_of(position.xz()).is_none()
                    } else {
                        grid.cell_of(destination.world.xz()).is_none()
                    }
            });
            let connector = off_map.map(|connection| {
                let inset = Vec2::splat(grid.cell_size_m * 0.5);
                let upper = grid.origin
                    + Vec2::new(grid.width as f32, grid.height as f32) * grid.cell_size_m;
                let xy = if grid.cell_of(connection.gate.xz()).is_some()
                    || grid.width == 0
                    || grid.height == 0
                {
                    connection.gate.xz()
                } else {
                    connection
                        .gate
                        .xz()
                        .clamp(grid.origin + inset, upper - inset)
                };
                (connection.inbound, Vec3::new(xy.x, connection.gate.y, xy.y))
            });
            let (start, end) = match connector {
                Some((true, gate)) => (gate, destination.world),
                Some((false, gate)) => (position, gate),
                None => (position, destination.world),
            };
            let result = animal_policy
                .agent_with_authored_collision_limits(agent, variant)
                .and_then(|(agent, maximum_ground_slope_radians)| {
                    let external_segment = connector.map(|(inbound, gate)| {
                        if inbound {
                            (position, gate)
                        } else {
                            (gate, destination.world)
                        }
                    });
                    if external_segment.is_some_and(|(from, to)| {
                        fence_policy.world_segment_blocks_actor(
                            from,
                            to,
                            agent.capabilities,
                            actor_types,
                        )
                    }) {
                        return Err(NavigationFailure::NoRoute);
                    }
                    let result = advance_route(
                        navigation,
                        tank_water,
                        |from, to| {
                            fence_crossing_index.crossing_fences(from, to).any(|fence| {
                                fence_policy.fence_blocks_actor(
                                    fence,
                                    agent.capabilities,
                                    actor_types,
                                )
                            })
                        },
                        |from, to| {
                            fence_policy.world_segment_blocks_actor(
                                from,
                                to,
                                agent.capabilities,
                                actor_types,
                            )
                        },
                        &agent,
                        mode.copied().unwrap_or_default(),
                        maximum_ground_slope_radians,
                        start,
                        end,
                        &mut route,
                        &mut active.scratch,
                        &mut active.search,
                        ITERATIONS_PER_SLICE,
                        LONG_PATH_ITERATIONS,
                    )?;
                    if result {
                        if let Some((inbound, gate)) = connector {
                            if inbound {
                                route.points.insert(0, gate);
                            } else {
                                route.points.push(destination.world);
                            }
                        }
                    }
                    Ok(result)
                });
            match result {
                Ok(false) => return,
                Ok(true) if agent.capabilities.contains(NavFlags::AMPHIBIOUS) => {
                    for point in &mut route.points {
                        super::spatial_index_and_docking_operations::fit_to_terrain(
                            LocomotionMode::Swim,
                            point,
                            &terrain_index,
                            &terrain_assets,
                            &terrain_chunks,
                        );
                    }
                }
                Ok(true) => {}
                Err(_) => route.clear(),
            }
            finished.scope(|results| results.push((destination.request_id, entity, result.err())));
        },
    );

    let mut completed: Vec<_> = finished.drain().collect();
    completed.sort_unstable_by_key(|(id, entity, _)| (*id, entity.to_bits()));
    for (request_id, entity, failure) in completed {
        let mut actor = commands.entity(entity);
        actor.remove::<PendingRouteSearch>();
        if let Some(reason) = failure {
            actor.remove::<Destination>();
            navigation_failures.write(NavigationFailed {
                entity,
                request_id,
                reason,
            });
        }
    }
}
