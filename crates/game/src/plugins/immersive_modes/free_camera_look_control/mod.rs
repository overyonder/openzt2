//! Look input for the authored photo camera.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeActionFlags;

use crate::plugins::{
    camera::{
        camera_control_message_types::{AimFreeCameraAt, PositionFreeCamera},
        camera_runtime_state_types::{CameraBounds, CameraMode, CameraTuning, ZooCamera},
    },
    input::input_types::DeviceControlAxes,
    photos::photo_capture_types::PhotoMode,
};

use super::{
    immersive_mode_input_policy_operations::interaction_action_flags_allow_required_mode_action_flags,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode},
};

pub(super) fn aim_active_free_immersive_camera(
    time: Res<Time<Real>>,
    control_axes: Res<DeviceControlAxes>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    active_modes: Query<(&ActiveImmersiveMode, Option<&AllowedInteractionActions>)>,
    cameras: Query<
        (
            &CameraMode,
            &CameraTuning,
            &CameraBounds,
            &Transform,
            Has<PhotoMode>,
        ),
        With<ZooCamera>,
    >,
    mut camera_position_requests: MessageWriter<PositionFreeCamera>,
    mut camera_aim_requests: MessageWriter<AimFreeCameraAt>,
) {
    if modal_input.0.is_some() {
        return;
    }
    let Ok((active, allowed_actions)) = active_modes.single() else {
        return;
    };
    if active.mode != ImmersiveMode::Photo {
        return;
    }
    let Ok((mode, camera_tuning, camera_bounds, camera_transform, photo)) =
        cameras.get(active.camera)
    else {
        return;
    };
    if *mode != CameraMode::Free || !photo {
        return;
    }
    let look_input = interaction_action_flags_allow_required_mode_action_flags(
        allowed_actions,
        ImmersiveModeActionFlags::LOOK,
    )
    .then_some(control_axes.look)
    .unwrap_or(Vec2::ZERO);
    let elapsed_seconds = time.delta_secs();
    if elapsed_seconds.is_finite()
        && elapsed_seconds > 0.0
        && camera_tuning.is_valid()
        && camera_bounds.is_valid()
        && look_input != Vec2::ZERO
    {
        let (yaw, pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);
        let yaw = (yaw - look_input.x * camera_tuning.turn_speed_rps * elapsed_seconds)
            .clamp(*camera_tuning.yaw.start(), *camera_tuning.yaw.end());
        let pitch = (pitch - look_input.y * camera_tuning.turn_speed_rps * elapsed_seconds)
            .clamp(*camera_tuning.pitch.start(), *camera_tuning.pitch.end());
        let rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        let mut translation = camera_transform.translation;
        translation.x = translation
            .x
            .clamp(camera_bounds.min.x, camera_bounds.max.x);
        translation.z = translation
            .z
            .clamp(camera_bounds.min.y, camera_bounds.max.y);
        camera_position_requests.write(PositionFreeCamera { world: translation });
        camera_aim_requests.write(AimFreeCameraAt {
            world: translation + rotation * Vec3::NEG_Z,
        });
    }
}
