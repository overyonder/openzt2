use avian3d::prelude::{LinearVelocity, RigidBody};
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};

use super::{
    locomotion_types::{
        Destination, DirectLocomotion, Docking, LocomotionMode, NavAgent, Route, Steering, Velocity,
    },
    spatial_index_and_docking_operations::fit_to_terrain,
};

pub(super) fn clear_planned_navigation_state_for_direct_locomotion(
    mut commands: Commands,
    mut direct_locomotion_agents: Query<
        (
            Entity,
            Option<&mut Route>,
            Option<&mut Velocity>,
            Option<&mut LinearVelocity>,
            Option<&mut Steering>,
        ),
        Added<DirectLocomotion>,
    >,
) {
    for (navigation_agent_entity, planned_route, local_velocity, avian_linear_velocity, steering) in
        &mut direct_locomotion_agents
    {
        if let Some(mut planned_route) = planned_route {
            planned_route.clear();
        }
        if let Some(mut local_velocity) = local_velocity {
            local_velocity.0 = Vec3::ZERO;
        }
        if let Some(mut avian_linear_velocity) = avian_linear_velocity {
            avian_linear_velocity.0 = Vec3::ZERO;
        }
        if let Some(mut steering) = steering {
            steering.desired_velocity = Vec3::ZERO;
        }
        commands
            .entity(navigation_agent_entity)
            .remove::<(Destination, Docking)>();
    }
}

pub(super) fn calculate_direct_locomotion_steering(
    mut direct_locomotion_agents: Query<(
        &NavAgent,
        &GlobalTransform,
        &DirectLocomotion,
        &mut Steering,
    )>,
) {
    for (navigation_agent, global_transform, direct_locomotion, mut steering) in
        &mut direct_locomotion_agents
    {
        let requested_local_axes = if direct_locomotion.local_axes.is_finite() {
            direct_locomotion.local_axes.clamp_length_max(1.0)
        } else {
            Vec2::ZERO
        };
        let global_forward_direction = global_transform.forward().as_vec3();
        let global_right_direction = global_transform.right().as_vec3();
        let mut desired_global_direction = global_forward_direction * requested_local_axes.y
            + global_right_direction * requested_local_axes.x;
        desired_global_direction.y = 0.0;
        steering.desired_velocity =
            desired_global_direction.normalize_or_zero() * navigation_agent.max_speed_mps;
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn integrate_non_physics_direct_locomotion(
    time: Res<Time<Fixed>>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut direct_locomotion_agents: Query<
        (
            &NavAgent,
            Option<&LocomotionMode>,
            &mut Transform,
            &mut Velocity,
            &Steering,
            &DirectLocomotion,
        ),
        Without<RigidBody>,
    >,
) {
    let fixed_delta_seconds = time.delta_secs();
    for (navigation_agent, locomotion_mode, mut local_transform, mut local_velocity, steering, _) in
        &mut direct_locomotion_agents
    {
        let requested_velocity_change = steering.desired_velocity - local_velocity.0;
        local_velocity.0 += requested_velocity_change
            .clamp_length_max(navigation_agent.acceleration_mps2 * fixed_delta_seconds);
        local_velocity.0 = local_velocity
            .0
            .clamp_length_max(navigation_agent.max_speed_mps);
        let previous_translation = local_transform.translation;
        local_transform.translation += local_velocity.0 * fixed_delta_seconds;

        let locomotion_mode = locomotion_mode.copied().unwrap_or_default();
        if locomotion_mode != LocomotionMode::Flight {
            fit_to_terrain(
                locomotion_mode,
                &mut local_transform.translation,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            );
            local_velocity.0.y = 0.0;
        }
        // The immersive controller owns look rotation. Strafing must not turn
        // the subject and thereby rotate the subject-relative camera.
        if !local_transform.translation.is_finite() {
            local_transform.translation = previous_translation;
            local_velocity.0 = Vec3::ZERO;
        }
    }
}
