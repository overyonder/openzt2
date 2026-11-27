use super::sampled_animation_motion_projection::{
    LocomotionAnimationSamples, LocomotionAnimationSource,
};
use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;

use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use crate::locomotion::spatial_index_and_docking_operations::advance_route_cursor;
use crate::locomotion::spatial_index_and_docking_operations::fit_to_terrain;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
/// Beyond this from the way it walks, an animal turns on the spot first.
const ANIMAL_TURN_IN_PLACE_RADIANS: f32 = std::f32::consts::FRAC_PI_3;

/// How close to its way an animal turns on the spot before it walks.
const ANIMAL_TURN_TOLERANCE_RADIANS: f32 = 10.0 * std::f32::consts::PI / 180.0;

/// How fast a walking animal turns towards its way.
const ANIMAL_WALKING_TURN_RATE_RADIANS_PER_SECOND: f32 = std::f32::consts::FRAC_PI_2;

pub(super) fn integrate_non_physics_planned_navigation_motion(
    mut commands: Commands,
    animation_samples: LocomotionAnimationSamples,
    time: Res<Time<Fixed>>,
    navigation_graph: Option<
        Res<openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph>,
    >,
    fences: FenceNavigationPolicy,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    pursuit_targets: Query<&GlobalTransform>,
    mut non_physics_navigation_agents: Query<
        (
            Entity,
            &NavAgent,
            Option<&LocomotionMode>,
            &mut Transform,
            &mut Velocity,
            &Steering,
            &mut Route,
            &Destination,
            Option<&Docking>,
            Option<&LocomotionAnimationSource>,
            Option<&openzt2_zoo_state::locomotion::locomotion_types::EntityPursuit>,
            Has<openzt2_zoo_state::animal_lifecycle::types::Animal>,
            Has<openzt2_zoo_state::locomotion::locomotion_types::TurnToHeading>,
        ),
        (Without<DirectLocomotion>, Without<RigidBody>),
    >,
    mut completed_arrivals: MessageWriter<Arrived>,
) {
    let fixed_delta_seconds = time.delta_secs();
    for (
        navigation_agent_entity,
        navigation_agent,
        locomotion_mode,
        mut local_transform,
        mut local_velocity,
        steering,
        mut planned_route,
        destination,
        docking,
        animation_source,
        pursuit,
        animal,
        turning,
    ) in &mut non_physics_navigation_agents
    {
        // An animal turning on the spot stays put until it faces its way.
        if turning {
            local_velocity.0 = Vec3::ZERO;
            continue;
        }
        let mut steering_velocity = steering.desired_velocity;
        // Swimmers and fliers turn as they go; turning clips are for ground.
        if animal && locomotion_mode.copied().unwrap_or_default() == LocomotionMode::Ground {
            let wanted = planned_route.current().map_or(Vec2::ZERO, |point| {
                (point - local_transform.translation).xz()
            });
            if wanted.length_squared() > 1.0e-6 {
                let facing = local_transform.rotation * Vec3::Z;
                let error = (wanted.x.atan2(wanted.y) - facing.x.atan2(facing.z)
                    + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                // A way far round, such as a destination behind, is faced on
                // the spot with turning clips first, as `zt.exe` plans a turn
                // before a walk; a smaller one is taken while walking, at a
                // limited rate, rather than at once.
                if error.abs() > ANIMAL_TURN_IN_PLACE_RADIANS {
                    commands.entity(navigation_agent_entity).insert(
                        openzt2_zoo_state::locomotion::locomotion_types::TurnToHeading {
                            heading_radians: wanted.x.atan2(wanted.y),
                            tolerance_radians: ANIMAL_TURN_TOLERANCE_RADIANS,
                        },
                    );
                    local_velocity.0 = Vec3::ZERO;
                    continue;
                }
                let step = ANIMAL_WALKING_TURN_RATE_RADIANS_PER_SECOND * fixed_delta_seconds;
                let heading = Quat::from_rotation_y(error.clamp(-step, step)) * facing;
                steering_velocity = Vec3::new(heading.x, 0.0, heading.z).normalize_or_zero()
                    * steering.desired_velocity.length();
            }
        }
        let playback = animation_samples.get(navigation_agent_entity, animation_source);
        let animation_mean_speed = animation_source
            .map_or(navigation_agent.max_speed_mps, |source| {
                source.mean_speed_mps
            });
        let (desired_velocity, sampled_rotation) = super::sampled_animation_motion_projection::project_sampled_animation_motion_to_steering(steering_velocity, animation_mean_speed, fixed_delta_seconds, &local_transform, playback);
        let requested_velocity_change = desired_velocity - local_velocity.0;
        local_velocity.0 += requested_velocity_change
            .clamp_length_max(navigation_agent.acceleration_mps2 * fixed_delta_seconds);
        if playback.is_none_or(|(motion, controller)| !motion.owns_playback(controller)) {
            local_velocity.0 = local_velocity
                .0
                .clamp_length_max(navigation_agent.max_speed_mps);
        }
        let previous_translation = local_transform.translation;
        let next_translation = previous_translation + local_velocity.0 * fixed_delta_seconds;
        if fences.gate_blocks_movement(
            navigation_agent_entity,
            navigation_agent.capabilities,
            previous_translation,
            next_translation,
        ) {
            local_velocity.0 = Vec3::ZERO;
        } else {
            local_transform.translation = next_translation;
        }

        let locomotion_mode = locomotion_mode.copied().unwrap_or_default();
        if locomotion_mode != LocomotionMode::Flight {
            let path_height = navigation_graph.as_deref().and_then(|graph| {
                let aim_height = pursuit
                    .and_then(|pursuit| pursuit_targets.get(pursuit.target).ok())
                    .map(|target| target.translation().y)
                    .or_else(|| planned_route.current().map(|point| point.y))
                    .unwrap_or(local_transform.translation.y);
                super::spatial_index_and_docking_operations::connected_path_surface_height(
                    graph.terrain_derived_navigation_graph(),
                    previous_translation,
                    Vec3::new(
                        local_transform.translation.x,
                        aim_height,
                        local_transform.translation.z,
                    ),
                    locomotion_mode,
                )
            });
            if let Some(height) = path_height {
                local_transform.translation.y = height;
            } else {
                fit_to_terrain(
                    locomotion_mode,
                    &mut local_transform.translation,
                    &terrain_index,
                    &terrain_assets,
                    &terrain_chunks,
                );
            }
            local_velocity.0.y = 0.0;
        }
        let horizontal_velocity = Vec2::new(local_velocity.0.x, local_velocity.0.z);
        if horizontal_velocity.length_squared() > 0.0001 {
            let direction =
                if playback.is_some_and(|(motion, controller)| motion.owns_playback(controller)) {
                    steering_velocity.xz()
                } else {
                    horizontal_velocity
                };
            local_transform.rotation =
                Quat::from_rotation_y(direction.x.atan2(direction.y)) * sampled_rotation;
        }
        if !local_transform.translation.is_finite() {
            local_transform.translation = previous_translation;
            local_velocity.0 = Vec3::ZERO;
        }

        advance_route_cursor(
            &mut planned_route,
            local_transform.translation,
            locomotion_mode,
            destination.arrival_radius_m,
            navigation_agent.radius_m,
        );
        if !planned_route.points.is_empty()
            && planned_route.cursor >= planned_route.points.len()
            && docking.is_none()
        {
            local_velocity.0 = Vec3::ZERO;
            planned_route.clear();
            commands
                .entity(navigation_agent_entity)
                .remove::<Destination>();
            completed_arrivals.write(Arrived {
                entity: navigation_agent_entity,
                request_id: destination.request_id,
                target: None,
            });
        }
    }
}
