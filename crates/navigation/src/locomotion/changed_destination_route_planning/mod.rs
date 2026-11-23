use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
    utils::Parallel,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
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

#[derive(Component)]
pub(super) struct WaitingRouteSearch;

#[derive(Component)]
#[component(on_remove = recycle_route_search_scratch)]
pub(super) struct ActiveRouteSearch {
    search: RouteSearch,
    scratch: RouteScratch,
}

#[derive(Resource, Default)]
pub(super) struct ReusableRouteSearchScratch(Vec<RouteScratch>);

fn recycle_route_search_scratch(mut world: DeferredWorld, context: HookContext) {
    let Some(mut active) = world.get_mut::<ActiveRouteSearch>(context.entity) else {
        return;
    };
    let scratch = std::mem::take(&mut active.scratch);
    if let Some(mut reusable) = world.get_resource_mut::<ReusableRouteSearchScratch>() {
        reusable.0.push(scratch);
    }
}

type PlannedMovement = (
    Without<DirectLocomotion>,
    Without<EntityPursuit>,
    Without<BehaviorEvasionSteering>,
);

#[allow(clippy::type_complexity)]
pub(super) fn queue_changed_navigation_destinations(
    mut commands: Commands,
    mut agents: Query<
        (
            Entity,
            &Destination,
            &mut Route,
            Option<&RestoredNavigationRoute>,
            Has<WaitingRouteSearch>,
            Option<&mut ActiveRouteSearch>,
        ),
        (Changed<Destination>, With<NavAgent>, PlannedMovement),
    >,
    stale_connections: Query<Entity, (With<EntranceNavigationConnection>, Without<Destination>)>,
    cancelled: Query<
        Entity,
        (
            Or<(With<WaitingRouteSearch>, With<ActiveRouteSearch>)>,
            Or<(
                Without<Destination>,
                Without<NavAgent>,
                Without<Route>,
                Without<WorldMember>,
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
        commands
            .entity(entity)
            .remove::<(WaitingRouteSearch, ActiveRouteSearch)>();
    }
    for (entity, destination, mut route, restored, waiting, active) in &mut agents {
        if let Some(restored) = restored {
            commands.entity(entity).remove::<RestoredNavigationRoute>();
            if restored.request_id == destination.request_id && !route.points.is_empty() {
                commands
                    .entity(entity)
                    .remove::<(WaitingRouteSearch, ActiveRouteSearch)>();
                continue;
            }
        }
        route.clear();
        if let Some(mut active) = active {
            active.search = RouteSearch::default();
        } else if !waiting {
            commands.entity(entity).insert(WaitingRouteSearch);
        }
    }
}

pub(super) fn admit_waiting_route_searches(
    mut commands: Commands,
    roots: Query<(&ActiveTerrainDerivedNavigationGraph, &LocomotionCapacity)>,
    active: Query<(), With<ActiveRouteSearch>>,
    waiting: Query<(Entity, &WorldMember, &Destination), With<WaitingRouteSearch>>,
    mut reusable: ResMut<ReusableRouteSearchScratch>,
) {
    let free = MAX_ACTIVE_ROUTE_SEARCHES.saturating_sub(active.iter().count());
    if free == 0 {
        return;
    }

    let mut next = Vec::with_capacity(free);
    for (entity, member, destination) in &waiting {
        let Ok((graph, capacity)) = roots.get(member.root) else {
            continue;
        };
        if !capacity.is_valid() {
            continue;
        }
        let key = (destination.request_id, entity.to_bits());
        let index = next.partition_point(|(earlier, _, _, _)| *earlier < key);
        if index == free {
            continue;
        }
        if next.len() == free {
            let _ = next.pop();
        }
        next.insert(
            index,
            (
                key,
                entity,
                graph
                    .terrain_derived_navigation_graph()
                    .navigation_node_count(),
                capacity.max_route_points,
            ),
        );
    }

    for (_, entity, nodes, point_capacity) in next {
        let mut scratch = reusable.0.pop().unwrap_or_default();
        scratch.ensure_capacity(nodes, point_capacity);
        commands
            .entity(entity)
            .remove::<WaitingRouteSearch>()
            .insert(ActiveRouteSearch {
                search: RouteSearch::default(),
                scratch,
            });
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn advance_active_navigation_searches(
    mut commands: Commands,
    roots: Query<(
        Ref<ActiveTerrainDerivedNavigationGraph>,
        &LocomotionCapacity,
        Option<Ref<TankNavigationWater>>,
        &SpatialGrid,
        &TerrainIndex,
    )>,
    fence_crossing_index: FenceNavigationCrossingQueries,
    animal_policy: AnimalNavigationPolicy,
    fence_policy: FenceNavigationPolicy,
    mut agents: Query<
        (
            Entity,
            &NavAgent,
            Ref<WorldMember>,
            Option<&AnimalVariant>,
            &GlobalTransform,
            &Destination,
            Option<&LocomotionMode>,
            &mut Route,
            &mut ActiveRouteSearch,
            Option<&EntranceNavigationConnection>,
        ),
        PlannedMovement,
    >,
    mut navigation_failures: MessageWriter<NavigationFailed>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) {
    let mut finished = Parallel::<Vec<(u64, Entity, Option<NavigationFailure>)>>::default();
    agents.par_iter_mut().for_each(
        |(
            entity,
            agent,
            member,
            variant,
            transform,
            destination,
            mode,
            mut route,
            mut active,
            connection,
        )| {
            let Ok((graph, capacity, tank_water, grid, terrain_index)) = roots.get(member.root)
            else {
                return;
            };
            let navigation = graph.terrain_derived_navigation_graph();
            let active = &mut *active;
            if graph.is_changed()
                || member.is_changed()
                || fence_crossing_index.is_changed(member.root)
                || tank_water.as_ref().is_some_and(DetectChanges::is_changed)
            {
                active.search = RouteSearch::default();
                active.scratch.ensure_capacity(
                    navigation.navigation_node_count(),
                    capacity.max_route_points,
                );
            }
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
                            member.root,
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
                        tank_water.as_deref(),
                        |from, to| {
                            fence_crossing_index
                                .crossing_fences(member.root, from, to)
                                .any(|fence| {
                                    fence_policy.fence_blocks_actor(
                                        fence,
                                        agent.capabilities,
                                        actor_types,
                                    )
                                })
                        },
                        |from, to| {
                            fence_policy.world_segment_blocks_actor(
                                member.root,
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
                            terrain_index,
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
        actor.remove::<ActiveRouteSearch>();
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
