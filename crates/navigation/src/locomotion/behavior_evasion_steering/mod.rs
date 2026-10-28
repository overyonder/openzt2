use avian3d::prelude::{Collider, ShapeCastConfig, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use openzt2_zoo_state::{
    animal_lifecycle::types::AnimalVariant,
    behavior_task_execution_types::BehaviorEvasionSteering,
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Destination, LocomotionMode, NavAgent, Route,
    },
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};

use super::{
    animal_navigation_policy::AnimalNavigationPolicy,
    fence_navigation_policy::FenceNavigationPolicy, pathfinding::straight_line_sweep_admits_agent,
};

/// Evasion supplies a fresh local direction, with obstacle avoidance taking
/// precedence. Route following and motion integration consume its one-point
/// route exactly as they consume direct pursuit.
pub(super) fn project_behavior_evasion_steering(
    graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    surfaces: DrySurfaceEligibility,
    animal_policy: AnimalNavigationPolicy,
    fences: FenceNavigationPolicy,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    terrain: Query<(), With<openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk>>,
    mut actors: Query<(
        Entity,
        &BehaviorEvasionSteering,
        &GlobalTransform,
        &NavAgent,
        Option<&AnimalVariant>,
        Option<&LocomotionMode>,
        &mut Destination,
        &mut Route,
    )>,
) {
    let Some(graph) = graph else {
        return;
    };
    for (actor, evasion, transform, agent, variant, mode, mut destination, mut route) in &mut actors
    {
        if destination.request_id != evasion.request_id {
            continue;
        }
        let Ok((agent, slope)) = animal_policy.agent_with_authored_collision_limits(agent, variant)
        else {
            continue;
        };
        let position = transform.translation();
        let mode = mode.copied().unwrap_or_default();
        let lookahead = evasion
            .obstacle_radius_m
            .max(agent.radius_m * 2.0)
            .max(agent.max_speed_mps);
        let clear = |direction: Vec2| {
            let endpoint = position + Vec3::new(direction.x, 0.0, direction.y) * lookahead;
            let medium = surfaces.on_land(endpoint.xz());
            !(evasion.avoid_water && medium != Some(true)
                || evasion.avoid_land && medium != Some(false))
                && straight_line_sweep_admits_agent(
                    graph.terrain_derived_navigation_graph(),
                    &agent,
                    mode,
                    slope,
                    position,
                    endpoint,
                    None,
                )
                && !fences.world_segment_blocks_actor(
                    position,
                    endpoint,
                    agent.capabilities,
                    fences.actor_types.actor_types(actor),
                )
                && !fences.gate_blocks_movement(actor, agent.capabilities, position, endpoint)
        };
        let forward = evasion.direction.normalize_or_zero();
        let cast_direction = Dir3::new(Vec3::new(forward.x, 0.0, forward.y)).ok();
        let collision = cast_direction
            .filter(|_| evasion.obstacle_radius_m > 0.0)
            .and_then(|direction| {
                spatial_query.cast_shape_predicate(
                    &Collider::sphere(agent.radius_m.max(0.01)),
                    position + Vec3::Y * (agent.radius_m.max(0.01) + 0.01),
                    Quat::IDENTITY,
                    direction,
                    &ShapeCastConfig {
                        ignore_origin_penetration: true,
                        ..ShapeCastConfig::from_max_distance(evasion.obstacle_radius_m)
                    },
                    &SpatialQueryFilter::from_excluded_entities([actor]),
                    &|entity| {
                        !terrain.contains(entity)
                            && !parents
                                .iter_ancestors(entity)
                                .any(|parent| parent == actor || terrain.contains(parent))
                    },
                )
            });
        let avoidance = collision.map_or(Vec2::ZERO, |hit| hit.normal1.xz().normalize_or_zero());
        let avoidance = if avoidance == Vec2::ZERO && !clear(forward) {
            -forward
        } else {
            avoidance
        };
        let combined = (evasion.direction * 0.1 + avoidance).normalize_or_zero();
        let direction = if clear(combined) {
            combined
        } else {
            Vec2::ZERO
        };
        let point = position + Vec3::new(direction.x, 0.0, direction.y) * lookahead;
        route.clear();
        if direction != Vec2::ZERO {
            route.points.push(point);
        }
        destination.bypass_change_detection().world = point;
    }
}
