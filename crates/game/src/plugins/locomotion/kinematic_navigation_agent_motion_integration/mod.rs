use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
};

use super::{
    locomotion_types::{
        Arrived, Destination, DirectLocomotion, Docking, LocomotionMode, NavAgent, Route, Steering,
        Velocity,
    },
    spatial_index_and_docking_operations::{advance_route_cursor, fit_to_terrain},
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn integrate_non_physics_planned_navigation_motion(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
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
    ) in &mut non_physics_navigation_agents
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
        let horizontal_velocity = Vec2::new(local_velocity.0.x, local_velocity.0.z);
        if horizontal_velocity.length_squared() > 0.0001 {
            local_transform.rotation =
                Quat::from_rotation_y(horizontal_velocity.x.atan2(horizontal_velocity.y));
        }
        if !local_transform.translation.is_finite() {
            local_transform.translation = previous_translation;
            local_velocity.0 = Vec3::ZERO;
        }

        advance_route_cursor(
            &mut planned_route,
            local_transform.translation,
            destination.arrival_radius_m,
            navigation_agent.radius_m,
        );
        if planned_route.cursor >= planned_route.points.len() && docking.is_none() {
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
