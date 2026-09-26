use avian3d::prelude::{AngularVelocity, LinearVelocity, RigidBody};
use bevy::prelude::*;

use super::{
    locomotion_types::{
        Arrived, Destination, DirectLocomotion, Docking, LocomotionMode, NavAgent, Route, Steering,
    },
    spatial_index_and_docking_operations::advance_route_cursor,
};

/// Applies steering through Avian velocity; physics handles movement and contacts.
#[allow(clippy::type_complexity)]
pub(super) fn drive_avian_navigation_agent_velocities_and_complete_routes(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut avian_navigation_agents: Query<(
        Entity,
        &NavAgent,
        &GlobalTransform,
        &RigidBody,
        &mut LinearVelocity,
        &mut AngularVelocity,
        &Steering,
        Option<&LocomotionMode>,
        Option<&mut Route>,
        Option<&Destination>,
        Option<&Docking>,
        Has<DirectLocomotion>,
    )>,
    mut completed_arrivals: MessageWriter<Arrived>,
) {
    let fixed_delta_seconds = time.delta_secs().max(f32::EPSILON);
    for (
        navigation_agent_entity,
        navigation_agent,
        global_transform,
        rigid_body,
        mut linear_velocity,
        mut angular_velocity,
        steering,
        locomotion_mode,
        planned_route,
        destination,
        docking,
        direct_locomotion,
    ) in &mut avian_navigation_agents
    {
        if !matches!(*rigid_body, RigidBody::Dynamic | RigidBody::Kinematic) {
            linear_velocity.0 = Vec3::ZERO;
            angular_velocity.0 = Vec3::ZERO;
            continue;
        }

        // Route steering is not evaluated without a destination. Its retained
        // value must not restart movement after arrival or docking completes.
        let mut desired_velocity = if destination.is_some() || direct_locomotion {
            steering
                .desired_velocity
                .clamp_length_max(navigation_agent.max_speed_mps)
        } else {
            Vec3::ZERO
        };
        let preserve_vertical_physics_velocity = matches!(
            locomotion_mode.copied().unwrap_or_default(),
            LocomotionMode::Ground | LocomotionMode::Vehicle
        );
        if preserve_vertical_physics_velocity {
            // Horizontal locomotion intent must not cancel gravity or Avian's
            // vertical contact response on dynamic agents.
            desired_velocity.y = linear_velocity.y;
        }
        let requested_velocity_change = desired_velocity - linear_velocity.0;
        linear_velocity.0 += requested_velocity_change
            .clamp_length_max(navigation_agent.acceleration_mps2 * fixed_delta_seconds);
        if preserve_vertical_physics_velocity {
            let horizontal_velocity = Vec2::new(linear_velocity.x, linear_velocity.z)
                .clamp_length_max(navigation_agent.max_speed_mps);
            linear_velocity.x = horizontal_velocity.x;
            linear_velocity.z = horizontal_velocity.y;
        } else {
            linear_velocity.0 = linear_velocity
                .0
                .clamp_length_max(navigation_agent.max_speed_mps);
        }

        let horizontal_velocity = Vec2::new(linear_velocity.x, linear_velocity.z);
        if !direct_locomotion && horizontal_velocity.length_squared() > 0.0001 {
            let desired_forward_direction =
                Vec3::new(horizontal_velocity.x, 0.0, horizontal_velocity.y).normalize();
            // Converted authored models face +Z, matching kinematic navigation
            // and docking. Bevy's `forward()` is the camera's -Z convention.
            let current_forward_direction = global_transform.back().as_vec3();
            let yaw_error_radians = current_forward_direction
                .cross(desired_forward_direction)
                .y
                .atan2(current_forward_direction.dot(desired_forward_direction));
            angular_velocity.y = yaw_error_radians / fixed_delta_seconds;
        } else {
            angular_velocity.y = 0.0;
        }

        let (Some(mut planned_route), Some(destination)) = (planned_route, destination) else {
            continue;
        };
        advance_route_cursor(
            &mut planned_route,
            global_transform.translation(),
            destination.arrival_radius_m,
            navigation_agent.radius_m,
        );
        if planned_route.cursor >= planned_route.points.len() && docking.is_none() {
            if preserve_vertical_physics_velocity {
                linear_velocity.x = 0.0;
                linear_velocity.z = 0.0;
            } else {
                linear_velocity.0 = Vec3::ZERO;
            }
            angular_velocity.y = 0.0;
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
