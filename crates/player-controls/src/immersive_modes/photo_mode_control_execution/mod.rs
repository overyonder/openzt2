use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_interface_state::camera::camera_runtime_state_types::{
    CameraDefinition, CameraMode, ZooCamera,
};
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::photos::photo_capture_types::CapturePhotoRequest;
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_interface_state::photos::photo_capture_types::PhotoView;
use openzt2_zoo_state::{
    terrain::{
        dry_surface_eligibility::DrySurfaceEligibility,
        terrain_water_view_state::TerrainWaterViewState,
    },
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

pub(super) fn project_live_photo_camera_view(
    mut cameras: Query<
        (&Transform, &CameraDefinition, &CameraMode, &mut PhotoView),
        (With<ZooCamera>, With<PhotoMode>),
    >,
    vehicles: Query<(), With<TransportVehicle>>,
    water: Option<Res<TerrainWaterViewState>>,
    terrain: DrySurfaceEligibility,
    world_definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
) {
    let Ok((pose, selected, mode, mut view)) = cameras.single_mut() else {
        return;
    };
    let underwater = water.as_ref().and_then(|view| view.underwater_status()) == Some(true);
    let vehicle = mode
        .subject()
        .is_some_and(|subject| vehicles.contains(subject));
    let elevated = !underwater
        && !vehicle
        && world_definitions
            .get(&definition_assets)
            .and_then(|definitions| definitions.find_camera(selected.0))
            .zip(terrain.ground_height(pose.translation.xz()))
            .is_some_and(|(definition, ground)| {
                let fitting_surface = water
                    .as_ref()
                    .and_then(|view| view.surface_height())
                    .map_or(ground, |water| ground.max(water));
                pose.translation.y
                    > fitting_surface + definition.offset_m[1] + definition.ground_buffer_m
            });
    view.set_if_neq(PhotoView {
        elevated,
        underwater,
        vehicle,
    });
}
