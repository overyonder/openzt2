use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeActionFlags;
use openzt2_interface_state::{
    camera::camera_runtime_state_types::{CameraMode, CameraMouseLook, ZooCamera},
    input::input_types::{ActionSource, ActiveInputDevice},
    ui::{
        input_frame_capture_types::AuthoredModalInputCapture,
        pointer_capture_types::UiPointerCapture,
    },
};

use crate::{
    camera::{
        camera_execution_types::{CameraIntent, CameraTransition, CameraTuning},
        photo_zoom::PhotoCameraZoom,
    },
    input::input_execution_types::DeviceControlAxes,
};

use super::{
    first_person_view_angle_integration::integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds,
    immersive_control_execution_types::FirstPersonViewAngles,
    immersive_mode_entry_and_policy_operations::find_authored_immersive_mode_policy,
    immersive_mode_input_policy_operations::interaction_action_flags_allow_required_mode_action_flags,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::{
        ActiveImmersiveMode, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
    },
};

pub(super) fn aim_player_driven_camera(
    time: Res<Time<Real>>,
    control_axes: Res<DeviceControlAxes>,
    active_input: Res<ActiveInputDevice>,
    pointer: Res<UiPointerCapture>,
    modal: Res<AuthoredModalInputCapture>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    sessions: Query<(
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        Option<&AllowedInteractionActions>,
        Option<&ImmersiveSubject>,
        Option<&FirstPersonViewAngles>,
    )>,
    avatars: Query<&Transform, Without<ZooCamera>>,
    mut cameras: Query<
        (
            &CameraMode,
            &CameraTuning,
            &CameraIntent,
            &mut Transform,
            Option<&PhotoCameraZoom>,
            Has<CameraMouseLook>,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (active, camera, actions, subject, angles) in &sessions {
        let Ok((mode, tuning, intent, transform, zoom, mouse_look)) = cameras.get_mut(camera.0)
        else {
            continue;
        };
        if matches!(
            active.mode,
            ImmersiveMode::SuperStaff | ImmersiveMode::FossilSearch
        ) && *mode == CameraMode::FirstPerson
        {
            if let (Some(avatar), Some(angles)) = (
                subject.and_then(|subject| avatars.get(subject.0).ok()),
                angles,
            ) {
                let rotation = avatar.rotation
                    * Quat::from_euler(EulerRot::YXZ, angles.yaw, angles.pitch, 0.0);
                transform
                    .map_unchanged(|transform| &mut transform.rotation)
                    .set_if_neq(rotation);
            }
            continue;
        }
        if active.mode != ImmersiveMode::Photo || *mode != CameraMode::Free || modal.0.is_some() {
            continue;
        }
        let Some(policy) = find_authored_immersive_mode_policy(definitions, active.mode)
            .and_then(|policy| policy.first_person_movement.as_ref())
        else {
            continue;
        };
        let look = if interaction_action_flags_allow_required_mode_action_flags(
            actions,
            ImmersiveModeActionFlags::LOOK,
        ) {
            let look = match active_input.source {
                ActionSource::KeyboardMouse if mouse_look || !pointer.over_ui => {
                    control_axes.mouse_look
                }
                ActionSource::KeyboardMouse => Vec2::ZERO,
                _ => control_axes.look,
            };
            look.clamp_length_max(1.0)
        } else {
            Vec2::ZERO
        };
        let keyboard = intent.first_person_commands.turn_axis();
        if (look == Vec2::ZERO && keyboard == 0.0) || !tuning.is_valid() {
            continue;
        }
        let low = *tuning.distance.start();
        let high = *tuning.distance.end();
        let fraction = zoom
            .filter(|_| high > low)
            .map_or(0.0, |zoom| ((high - zoom.0) / (high - low)).clamp(0.0, 1.0));
        let speed = policy.turn_speed_out_rps
            + (policy.turn_speed_in_rps - policy.turn_speed_out_rps) * fraction;
        let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let mut pose = FirstPersonViewAngles { yaw, pitch };
        integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds(
            &mut pose,
            look,
            keyboard,
            [speed, policy.keyboard_rotate_speed_rps],
            &tuning.pitch,
            &tuning.yaw,
            time.delta_secs(),
        );
        transform
            .map_unchanged(|transform| &mut transform.rotation)
            .set_if_neq(Quat::from_euler(EulerRot::YXZ, pose.yaw, pose.pitch, 0.0));
    }
}
