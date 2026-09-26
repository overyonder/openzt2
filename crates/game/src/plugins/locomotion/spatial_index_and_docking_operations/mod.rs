use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;

use super::locomotion_types::{
    Arrived, Destination, DirectLocomotion, Docking, LocomotionMode, MotionProgress, NavAgent,
    NavigationFailed, NavigationFailure, Route, SpatialCell, SpatialGrid, Steering, Velocity,
    STUCK_TICKS,
};

pub(super) fn rebuild_spatial_grid(
    mut grid: ResMut<SpatialGrid>,
    mut agents: Query<(Entity, &GlobalTransform, &mut SpatialCell), With<NavAgent>>,
    mut failed: MessageWriter<NavigationFailed>,
) {
    if grid.width == 0 || grid.height == 0 {
        return;
    }
    grid.cell_offsets.fill(0);
    for (_, transform, _) in &agents {
        let position = transform.translation();
        if let Some(cell) = grid.cell_of(Vec2::new(position.x, position.z)) {
            grid.cell_offsets[cell as usize + 1] += 1;
        }
    }
    for index in 1..grid.cell_offsets.len() {
        grid.cell_offsets[index] += grid.cell_offsets[index - 1];
    }
    let count = *grid.cell_offsets.last().unwrap_or(&0) as usize;
    if count > grid.entities.capacity() {
        for (entity, _, _) in &agents {
            failed.write(NavigationFailed {
                entity,
                request_id: 0,
                reason: NavigationFailure::NoRoute,
            });
        }
        return;
    }
    grid.entities.clear();
    grid.entities.resize(count, Entity::PLACEHOLDER);
    let SpatialGrid {
        cell_offsets,
        write_cursors,
        ..
    } = &mut *grid;
    write_cursors.copy_from_slice(&cell_offsets[..write_cursors.len()]);
    for (entity, transform, mut spatial_cell) in &mut agents {
        let position = transform.translation();
        let Some(cell) = grid.cell_of(Vec2::new(position.x, position.z)) else {
            *spatial_cell = SpatialCell::OUTSIDE;
            continue;
        };
        let cursor = grid.write_cursors[cell as usize];
        grid.entities[cursor as usize] = entity;
        grid.write_cursors[cell as usize] = cursor + 1;
        spatial_cell.0 = cell;
    }
}

pub(crate) fn detect_stuck_agents(
    mut agents: Query<
        (
            &GlobalTransform,
            &Route,
            &mut Destination,
            &mut MotionProgress,
        ),
        Without<DirectLocomotion>,
    >,
) {
    for (transform, route, mut destination, mut progress) in &mut agents {
        if route.current().is_none() {
            progress.stationary_ticks = 0;
            continue;
        }
        let position = transform.translation();
        let moved = position.distance_squared(progress.last_position) > 0.0001;
        if moved {
            progress.stationary_ticks = 0;
        } else {
            progress.stationary_ticks = progress.stationary_ticks.saturating_add(1);
        }
        if progress.stationary_ticks >= STUCK_TICKS {
            destination.set_changed();
            progress.stationary_ticks = 0;
        }
        progress.last_position = position;
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
        if parent_transform
            .transform_point(transform.translation)
            .distance_squared(docking.point)
            > docking.radius_m * docking.radius_m
        {
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
    arrival_radius_m: f32,
    agent_radius_m: f32,
) {
    while let Some(point) = route.current() {
        let radius = if route.cursor + 1 == route.points.len() {
            arrival_radius_m.max(agent_radius_m * 0.25)
        } else {
            agent_radius_m.max(0.05)
        };
        if position.distance_squared(point) > radius * radius {
            break;
        }
        route.cursor += 1;
    }
}
