use bevy::prelude::*;

use openzt2_interface_state::camera::camera_runtime_state_types::{
    StandaloneCameraSubject, ZooCamera,
};
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::photos::photo_capture_types::CapturePhotoRequest;
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_interface_state::photos::photo_capture_types::PhotoView;
use openzt2_zoo_state::{
    terrain::terrain_water_view_state::TerrainWaterCameraContact,
    transport_tours::transport_vehicle_types::TransportVehicle,
};

use super::immersive_mode_input_policy_operations::interaction_action_flags_allow_requested_game_action;
use super::immersive_mode_input_policy_operations::request_cancelled_immersive_mode_exit;
use super::immersive_mode_message_types::ExitImmersiveMode;
use super::immersive_mode_policy_types::AllowedInteractionActions;
use super::immersive_mode_state_types::ActiveImmersiveMode;

#[allow(clippy::type_complexity)]
pub(super) fn capture_or_cancel_active_photo_mode(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
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
            GameAction::Confirm | GameAction::UseObject => {
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

pub(super) fn project_live_photo_camera_view(
    mut cameras: Query<
        (Option<&StandaloneCameraSubject>, &mut PhotoView),
        (With<ZooCamera>, With<PhotoMode>),
    >,
    vehicles: Query<(), With<TransportVehicle>>,
    water: Query<&TerrainWaterCameraContact>,
) {
    let Ok((subject, mut view)) = cameras.single_mut() else {
        return;
    };
    let underwater = water
        .single()
        .ok()
        .and_then(|contact| contact.underwater_status)
        == Some(true);
    let vehicle = subject.is_some_and(|subject| vehicles.contains(subject.0));
    // Exposure collection samples elevation from the camera's world pose.
    if view.underwater != underwater || view.vehicle != vehicle {
        view.underwater = underwater;
        view.vehicle = vehicle;
    }
}
