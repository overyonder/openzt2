use bevy::prelude::*;

use crate::{
    plugins::input::input_types::{ActionRequest, ActionSource, GameAction},
    plugins::{
        camera::camera_runtime_state_types::ZooCamera,
        photos::photo_capture_types::{CapturePhotoRequest, PhotoMode},
    },
};

use super::{
    immersive_mode_input_policy_operations::{
        interaction_action_flags_allow_requested_game_action, request_cancelled_immersive_mode_exit,
    },
    immersive_mode_message_types::ExitImmersiveMode,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::ActiveImmersiveMode,
};

#[allow(clippy::type_complexity)]
pub(super) fn capture_or_cancel_active_photo_mode(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    active_modes: Query<(
        Entity,
        &ActiveImmersiveMode,
        Option<&AllowedInteractionActions>,
    )>,
    photo_cameras: Query<Entity, (With<ZooCamera>, With<PhotoMode>)>,
    mut photo_capture_requests: MessageWriter<CapturePhotoRequest>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    let Ok((controller_entity, _, allowed_actions)) = active_modes.single() else {
        return;
    };
    let Ok(camera_entity) = photo_cameras.single() else {
        return;
    };
    for action_request in action_requests.read() {
        // The shipped keyboard contract is authored by the active `photomode`
        // hotkey document. Device-independent actions remain the native
        // controller path instead of making Enter a second keyboard shutter.
        if action_request.source == ActionSource::KeyboardMouse {
            continue;
        }
        if !interaction_action_flags_allow_requested_game_action(
            allowed_actions,
            action_request.action,
        ) {
            continue;
        }
        match action_request.action {
            GameAction::Confirm => {
                photo_capture_requests.write(CapturePhotoRequest {
                    camera: camera_entity,
                });
            }
            GameAction::Cancel => {
                request_cancelled_immersive_mode_exit(controller_entity, &mut exit_requests);
            }
            _ => {}
        }
    }
}
