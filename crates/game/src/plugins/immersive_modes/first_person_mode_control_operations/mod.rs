use bevy::prelude::*;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeActionFlags;

use crate::{
    plugins::input::input_types::{ActionRequest, ActionSource, GameAction},
    plugins::{
        camera::camera_runtime_state_types::{CameraMouseLook, CameraTuning, ZooCamera},
        input::input_types::DeviceControlAxes,
        locomotion::locomotion_types::DirectLocomotion,
    },
};

use super::{
    first_person_view_angle_integration::integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds,
    immersive_mode_control_types::FirstPersonViewAngles,
    immersive_mode_input_policy_operations::{
        interaction_action_flags_allow_requested_game_action,
        interaction_action_flags_allow_required_mode_action_flags,
        request_cancelled_immersive_mode_exit,
    },
    immersive_mode_message_types::ExitImmersiveMode,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::{ActiveImmersiveMode, ControlledEntity},
};

pub(super) fn apply_allowed_navigation_axes_to_directly_controlled_subject(
    control_axes: &DeviceControlAxes,
    active_immersive_mode: &ActiveImmersiveMode,
    allowed_interaction_actions: Option<&AllowedInteractionActions>,
    controller_entity: Entity,
    view_yaw_radians: f32,
    controlled_subjects: &mut Query<(&ControlledEntity, &mut DirectLocomotion)>,
) {
    let Some(subject_entity) = active_immersive_mode.subject else {
        return;
    };
    let Ok((ownership, mut direct_locomotion)) = controlled_subjects.get_mut(subject_entity) else {
        return;
    };
    if ownership.controller != controller_entity {
        return;
    }
    let mut allowed_local_axes = control_axes.pan.clamp_length_max(1.0);
    if !interaction_action_flags_allow_required_mode_action_flags(
        allowed_interaction_actions,
        ImmersiveModeActionFlags::STRAFE_LEFT
            .with_additional_flags(ImmersiveModeActionFlags::STRAFE_RIGHT),
    ) {
        allowed_local_axes.x = 0.0;
    }
    if !interaction_action_flags_allow_required_mode_action_flags(
        allowed_interaction_actions,
        ImmersiveModeActionFlags::MOVE_FORWARD
            .with_additional_flags(ImmersiveModeActionFlags::MOVE_BACK),
    ) {
        allowed_local_axes.y = 0.0;
    }
    // The camera adds view yaw to the subject transform. Apply the same yaw
    // to movement intent, leaving pitch out of ground navigation.
    direct_locomotion.local_axes = Mat2::from_angle(view_yaw_radians) * allowed_local_axes;
}

#[cfg(test)]
mod tests {
    use super::super::immersive_mode_state_types::ImmersiveMode;
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
        let mode = ActiveImmersiveMode {
            mode: ImmersiveMode::FirstPerson,
            subject: Some(subject),
            camera: controller,
            restore_camera_on_exit: true,
        };
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
                &mode,
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
    active_immersive_mode: &ActiveImmersiveMode,
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
        zoo_camera_tuning.get(active_immersive_mode.camera),
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
