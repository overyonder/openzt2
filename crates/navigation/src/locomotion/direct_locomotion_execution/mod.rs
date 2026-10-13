use avian3d::prelude::LinearVelocity;
use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;

use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use crate::locomotion::spatial_index_and_docking_operations::fit_to_terrain;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;

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
    fences: FenceNavigationPolicy,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    mut direct_locomotion_agents: Query<
        (
            Entity,
            &NavAgent,
            Option<&LocomotionMode>,
            &mut Transform,
            &mut Velocity,
            &Steering,
            &DirectLocomotion,
            Option<&WorldMember>,
        ),
        Without<RigidBody>,
    >,
) {
    let fixed_delta_seconds = time.delta_secs();
    for (
        entity,
        navigation_agent,
        locomotion_mode,
        mut local_transform,
        mut local_velocity,
        steering,
        _,
        world_member,
    ) in &mut direct_locomotion_agents
    {
        let requested_velocity_change = steering.desired_velocity - local_velocity.0;
        local_velocity.0 += requested_velocity_change
            .clamp_length_max(navigation_agent.acceleration_mps2 * fixed_delta_seconds);
        local_velocity.0 = local_velocity
            .0
            .clamp_length_max(navigation_agent.max_speed_mps);
        let previous_translation = local_transform.translation;
        let mut next_translation = previous_translation + local_velocity.0 * fixed_delta_seconds;
        // These actors have no Avian body, so world-edge colliders cannot
        // constrain them. Use their world's authored bounds at the motion owner.
        if let Some(member) = world_member {
            let Ok(bounds) = world_bounds.get(member.root) else {
                local_velocity.0 = Vec3::ZERO;
                continue;
            };
            let horizontal = next_translation.xz().clamp(bounds.min, bounds.max);
            if horizontal.x != next_translation.x {
                local_velocity.0.x = 0.0;
            }
            if horizontal.y != next_translation.z {
                local_velocity.0.z = 0.0;
            }
            next_translation.x = horizontal.x;
            next_translation.z = horizontal.y;
        }
        if fences.gate_blocks_movement(
            entity,
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
