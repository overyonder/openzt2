use avian3d::prelude::AngularVelocity;
use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;

use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::MotionProgress;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;
use openzt2_zoo_state::locomotion::locomotion_types::STUCK_TICKS;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialCell, SpatialCellMembers};

type SpatialAgentData = (
    Entity,
    &'static GlobalTransform,
    Option<&'static Transform>,
    Has<ChildOf>,
    &'static WorldMember,
    Option<&'static SpatialCell>,
);

#[allow(clippy::type_complexity)]
pub(super) fn project_agent_spatial_membership(
    mut commands: Commands,
    parallel: ParallelCommands,
    grids: Query<&SpatialGrid>,
    changed_grids: Query<(), Changed<SpatialGrid>>,
    agents: Query<SpatialAgentData, With<NavAgent>>,
    moved_agents: Query<
        SpatialAgentData,
        (
            With<NavAgent>,
            Or<(
                Changed<GlobalTransform>,
                Changed<Transform>,
                Changed<WorldMember>,
                Added<NavAgent>,
            )>,
        ),
    >,
    mut removed: RemovedComponents<NavAgent>,
) {
    for entity in removed.read() {
        if !agents.contains(entity) {
            commands.entity(entity).try_remove::<SpatialCell>();
        }
    }
    let project = |(entity, transform, local, parented, member, current): (
        Entity,
        &GlobalTransform,
        Option<&Transform>,
        bool,
        &WorldMember,
        Option<&SpatialCell>,
    )| {
        let position = local
            .filter(|_| !parented)
            .map_or_else(|| transform.translation(), |local| local.translation);
        let next = grids.get(member.root).ok().and_then(|grid| {
            grid.cell_of(position.xz())
                .and_then(|index| grid.cells.get(index as usize).copied())
        });
        if current.map(|cell| cell.0) != next {
            parallel.command_scope(|mut commands| {
                if let Some(next) = next {
                    commands.entity(entity).insert(SpatialCell(next));
                } else {
                    commands.entity(entity).remove::<SpatialCell>();
                }
            });
        }
    };
    if changed_grids.is_empty() {
        moved_agents.par_iter().for_each(project);
    } else {
        agents.par_iter().for_each(project);
    }
}

pub(super) fn order_changed_spatial_cell_members(
    mut cells: Query<&mut SpatialCellMembers, Changed<SpatialCellMembers>>,
) {
    cells.par_iter_mut().for_each(|mut members| {
        members
            .collection_mut_risky()
            .sort_unstable_by_key(|entity| entity.to_bits());
    });
}

#[derive(Component, Default)]
pub(super) struct RouteStallTimer(Timer);

pub(super) fn detect_stuck_agents(
    time: Res<Time<Fixed>>,
    mut agents: Query<
        (
            &GlobalTransform,
            &Route,
            &mut Destination,
            &mut MotionProgress,
            &mut RouteStallTimer,
        ),
        Without<DirectLocomotion>,
    >,
) {
    let step = time.timestep();
    let timeout = step.saturating_mul(u32::from(STUCK_TICKS));
    for (transform, route, mut destination, mut progress, mut stall) in &mut agents {
        if stall.is_added() || progress.is_changed() || stall.0.duration() != timeout {
            stall.0 = Timer::new(timeout, TimerMode::Once);
            stall
                .0
                .tick(step.saturating_mul(u32::from(progress.stationary_ticks)));
        }
        if route.current().is_none() {
            stall.0.reset();
            progress
                .map_unchanged(|progress| &mut progress.stationary_ticks)
                .set_if_neq(0);
            continue;
        }
        let position = transform.translation();
        let moved = position.distance_squared(progress.last_position) > 0.0001;
        if moved {
            stall.0.reset();
        } else if stall.0.tick(step).is_finished() {
            destination.set_changed();
            stall.0.reset();
        }
        let elapsed_ticks =
            u16::try_from(stall.0.elapsed().as_nanos() / step.as_nanos()).unwrap_or(u16::MAX);
        progress
            .reborrow()
            .map_unchanged(|progress| &mut progress.last_position)
            .set_if_neq(position);
        progress
            .map_unchanged(|progress| &mut progress.stationary_ticks)
            .set_if_neq(elapsed_ticks);
    }
}

pub(super) fn complete_docking(
    mut commands: Commands,
    mut dockers: Query<(
        Entity,
        &mut Transform,
        Option<&mut Velocity>,
        Option<&mut LinearVelocity>,
        Option<&mut AngularVelocity>,
        &Docking,
        Option<&ChildOf>,
        Option<&mut Route>,
        Option<&mut Steering>,
        Option<&LocomotionMode>,
    )>,
    targets: Query<&GlobalTransform>,
    mut arrived: MessageWriter<Arrived>,
    mut failed: MessageWriter<NavigationFailed>,
) {
    for (
        entity,
        mut transform,
        velocity,
        avian_velocity,
        angular_velocity,
        docking,
        parent,
        route,
        steering,
        locomotion_mode,
    ) in &mut dockers
    {
        if targets.get(docking.target).is_err() {
            commands.entity(entity).remove::<(Docking, Destination)>();
            failed.write(NavigationFailed {
                entity,
                request_id: docking.request_id,
                reason: NavigationFailure::TargetGone,
            });
            continue;
        }
        let parent_transform = match parent {
            Some(parent) => {
                let Ok(transform) = targets.get(parent.parent()) else {
                    continue;
                };
                *transform
            }
            None => GlobalTransform::IDENTITY,
        };
        let mut offset = parent_transform.transform_point(transform.translation) - docking.point;
        // Surface locomotion follows terrain height. Authored docking nodes may
        // offset the actor root vertically; apply that alignment on arrival.
        if locomotion_mode.copied().unwrap_or_default() != LocomotionMode::Flight {
            offset.y = 0.0;
        }
        if offset.length_squared() > docking.radius_m * docking.radius_m {
            continue;
        }
        transform.translation = parent_transform
            .affine()
            .inverse()
            .transform_point3(docking.point);
        let forward = Vec2::new(docking.forward.x, docking.forward.z).normalize_or_zero();
        if forward != Vec2::ZERO {
            transform.rotation = parent_transform.rotation().inverse()
                * Quat::from_rotation_y(forward.x.atan2(forward.y));
        }
        if let Some(mut velocity) = velocity {
            velocity.0 = Vec3::ZERO;
        }
        if let Some(mut velocity) = avian_velocity {
            velocity.0 = Vec3::ZERO;
        }
        if let Some(mut velocity) = angular_velocity {
            velocity.0 = Vec3::ZERO;
        }
        if let Some(mut route) = route {
            route.clear();
        }
        if let Some(mut steering) = steering {
            steering.desired_velocity = Vec3::ZERO;
        }
        commands.entity(entity).remove::<(Docking, Destination)>();
        arrived.write(Arrived {
            entity,
            request_id: docking.request_id,
            target: Some(docking.target),
        });
    }
}

pub(super) fn connected_path_surface_height(
    graph: &openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph,
    from: Vec3,
    to: Vec3,
    mode: LocomotionMode,
) -> Option<f32> {
    if !matches!(mode, LocomotionMode::Ground | LocomotionMode::Vehicle) {
        return None;
    }
    let start = graph.navigation_surface_at_world_position(from)?;
    let next = graph.continue_navigation_surface(start, to)?;
    graph.path_surface_at_node(next)?.height_at(to.xz())
}

pub(super) fn fit_to_terrain(
    mode: LocomotionMode,
    position: &mut Vec3,
    index: &TerrainIndex,
    assets: &Assets<TerrainAsset>,
    chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) {
    let world_xz = Vec2::new(position.x, position.z);
    let Some(entity) = terrain_chunk_at(index, world_xz) else {
        return;
    };
    let Ok((chunk, edited)) = chunks.get(entity) else {
        return;
    };
    let Some(asset) = assets.get(&chunk.asset) else {
        return;
    };
    let Some(sample) = sample_terrain(chunk, asset, edited, world_xz) else {
        return;
    };
    position.y = match mode {
        LocomotionMode::Swim => sample.water_height_m.unwrap_or(sample.height_m),
        LocomotionMode::Ground | LocomotionMode::Vehicle => sample.height_m,
        LocomotionMode::Flight => position.y,
    };
}

#[inline]
pub(super) fn advance_route_cursor(
    route: &mut Route,
    position: Vec3,
    locomotion_mode: LocomotionMode,
    arrival_radius_m: f32,
    agent_radius_m: f32,
) {
    while let Some(point) = route.current() {
        let radius = if route.cursor + 1 == route.points.len() {
            arrival_radius_m.max(agent_radius_m * 0.25)
        } else {
            agent_radius_m.max(0.05)
        };
        let mut offset = point - position;
        // Surface steering follows terrain height rather than waypoint height.
        if locomotion_mode != LocomotionMode::Flight {
            offset.y = 0.0;
        }
        if offset.length_squared() > radius * radius {
            break;
        }
        route.cursor += 1;
    }
}
