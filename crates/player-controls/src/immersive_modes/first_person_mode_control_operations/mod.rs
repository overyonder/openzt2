use bevy::prelude::*;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeActionFlags;

use crate::camera::camera_execution_types::CameraTuning;
use crate::input::input_execution_types::DeviceControlAxes;
use openzt2_interface_state::camera::camera_runtime_state_types::CameraMouseLook;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;

use super::first_person_view_angle_integration::integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds;
use super::immersive_mode_input_policy_operations::interaction_action_flags_allow_requested_game_action;
use super::immersive_mode_input_policy_operations::interaction_action_flags_allow_required_mode_action_flags;
use super::immersive_mode_input_policy_operations::request_cancelled_immersive_mode_exit;
use super::immersive_mode_message_types::ExitImmersiveMode;
use super::immersive_mode_policy_types::AllowedInteractionActions;
use super::immersive_mode_state_types::ControlledEntity;
use crate::immersive_modes::immersive_control_execution_types::FirstPersonViewAngles;

pub(super) fn apply_allowed_navigation_axes_to_directly_controlled_subject(
    control_axes: &DeviceControlAxes,
    subject: Option<Entity>,
    allowed_interaction_actions: Option<&AllowedInteractionActions>,
    controller_entity: Entity,
    view_yaw_radians: f32,
    controlled_subjects: &mut Query<(&ControlledEntity, &mut DirectLocomotion)>,
) {
    let Some(subject_entity) = subject else {
        return;
    };
    let Ok((ownership, mut direct_locomotion)) = controlled_subjects.get_mut(subject_entity) else {
        return;
    };
    if ownership.controller != controller_entity {
        return;
    }
    let allowed_axis = |value, negative, positive| {
        let direction = if value < 0.0 { negative } else { positive };
        if interaction_action_flags_allow_required_mode_action_flags(
            allowed_interaction_actions,
            direction,
        ) {
            value
        } else {
            0.0
        }
    };
    let allowed_local_axes = Vec2::new(
        allowed_axis(
            control_axes.pan.x,
            ImmersiveModeActionFlags::STRAFE_LEFT,
            ImmersiveModeActionFlags::STRAFE_RIGHT,
        ),
        allowed_axis(
            control_axes.pan.y,
            ImmersiveModeActionFlags::MOVE_BACK,
            ImmersiveModeActionFlags::MOVE_FORWARD,
        ),
    )
    .clamp_length_max(1.0);
    direct_locomotion
        .map_unchanged(|locomotion| &mut locomotion.local_axes)
        .set_if_neq(Mat2::from_angle(view_yaw_radians) * allowed_local_axes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    #[test]
    fn forward_movement_follows_subject_relative_camera_yaw() {
        let mut world = World::new();
        let controller = world.spawn_empty().id();
        let subject = world
            .spawn((
                ControlledEntity { controller },
                DirectLocomotion {
                    local_axes: Vec2::ZERO,
                },
            ))
            .id();
        let controls = DeviceControlAxes {
            pan: Vec2::Y,
            ..DeviceControlAxes::default()
        };
        let mut state =
            SystemState::<Query<(&ControlledEntity, &mut DirectLocomotion)>>::new(&mut world);
        for yaw in [
            0.0,
            std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2,
        ] {
            apply_allowed_navigation_axes_to_directly_controlled_subject(
                &controls,
                Some(subject),
                None,
                controller,
                yaw,
                &mut state.get_mut(&mut world).expect("registered control query"),
            );
            let axes = world
                .get::<DirectLocomotion>(subject)
                .expect("controlled subject")
                .local_axes;
            let movement = Vec3::new(axes.x, 0.0, -axes.y);
            assert!(movement.abs_diff_eq(Quat::from_rotation_y(yaw) * Vec3::NEG_Z, 1.0e-6));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_allowed_look_rotation_and_cancellation_to_first_person_view<M: Component>(
    action_requests: &mut MessageReader<ActionRequest>,
    control_axes: &DeviceControlAxes,
    elapsed_seconds: f32,
    camera: Entity,
    allowed_interaction_actions: Option<&AllowedInteractionActions>,
    controller_entity: Entity,
    active_action_source: ActionSource,
    first_person_view_angles: &mut Query<&mut FirstPersonViewAngles, With<M>>,
    zoo_camera_tuning: &Query<(&CameraTuning, Has<CameraMouseLook>), With<ZooCamera>>,
    exit_requests: &mut MessageWriter<ExitImmersiveMode>,
) {
    let mut digital_horizontal_turn = 0.0;
    for action_request in action_requests.read() {
        if !interaction_action_flags_allow_requested_game_action(
            allowed_interaction_actions,
            action_request.action,
        ) {
            continue;
        }
        match action_request.action {
            GameAction::RotateLeft => digital_horizontal_turn -= 1.0,
            GameAction::RotateRight => digital_horizontal_turn += 1.0,
            GameAction::Cancel => {
                request_cancelled_immersive_mode_exit(controller_entity, exit_requests);
            }
            _ => {}
        }
    }
    let (Ok(mut view_angles), Ok((camera_tuning, camera_has_mouse_look))) = (
        first_person_view_angles.single_mut(),
        zoo_camera_tuning.get(camera),
    ) else {
        return;
    };
    let analog_look_is_enabled =
        active_action_source != ActionSource::KeyboardMouse || camera_has_mouse_look;
    let allowed_look_input = if analog_look_is_enabled
        && interaction_action_flags_allow_required_mode_action_flags(
            allowed_interaction_actions,
            ImmersiveModeActionFlags::LOOK,
        ) {
        control_axes.look.clamp_length_max(1.0)
    } else {
        Vec2::ZERO
    };
    integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds(
        &mut view_angles,
        allowed_look_input,
        digital_horizontal_turn,
        camera_tuning.turn_speed_rps,
        &camera_tuning.pitch,
        &camera_tuning.yaw,
        elapsed_seconds,
    );
}
