use super::immersive_mode_state_types::{ImmersiveCamera, ImmersiveSubject};
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::camera::camera_runtime_state_types::CameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::CameraMouseLook;
use openzt2_interface_state::camera::camera_runtime_state_types::StandaloneCameraSubject;
use crate::camera::camera_execution_types::{CameraIntent, CameraTuning};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;

use super::first_person_mode_control_operations::apply_allowed_look_rotation_and_cancellation_to_first_person_view;
use super::immersive_mode_entry_and_policy_operations::find_authored_immersive_mode_policy;
use super::immersive_mode_message_types::ExitImmersiveMode;
use super::immersive_mode_policy_types::AllowedInteractionActions;
use super::immersive_mode_state_types::ActiveImmersiveMode;
use super::immersive_mode_state_types::ControlledEntity;
use super::immersive_mode_state_types::ImmersiveMode;
use crate::immersive_modes::immersive_control_execution_types::FirstPersonControl;
use crate::immersive_modes::immersive_control_execution_types::FirstPersonViewAngles;
use crate::immersive_modes::immersive_control_execution_types::GuestViewControl;

pub(super) fn cancel_guest_view(
    mut actions: MessageReader<ActionRequest>,
    modal: Res<openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture>,
    controller: Single<(Entity, Option<&AllowedInteractionActions>), With<GuestViewControl>>,
    mut exits: MessageWriter<ExitImmersiveMode>,
) {
    use super::immersive_mode_input_policy_operations::{
        interaction_action_flags_allow_requested_game_action, request_cancelled_immersive_mode_exit,
    };
    use openzt2_interface_state::input::input_types::GameAction;

    let (entity, allowed) = controller.into_inner();
    let cancel = actions
        .read()
        .any(|action| action.action == GameAction::Cancel);
    actions.clear();
    if cancel
        && modal.0.is_none()
        && interaction_action_flags_allow_requested_game_action(allowed, GameAction::Cancel)
    {
        request_cancelled_immersive_mode_exit(entity, &mut exits);
    }
}

pub(super) fn apply_first_person_look_and_cancellation(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    control_axes: Res<crate::input::input_execution_types::DeviceControlAxes>,
    active_input: Res<openzt2_interface_state::input::input_types::ActiveInputDevice>,
    time: Res<Time<Real>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active_modes: Query<
        (
            Entity,
            &ActiveImmersiveMode,
            &ImmersiveCamera,
            Option<&AllowedInteractionActions>,
        ),
        With<FirstPersonControl>,
    >,
    mut view_angles: Query<&mut FirstPersonViewAngles, With<FirstPersonControl>>,
    camera_tuning: Query<(&CameraTuning, Has<CameraMouseLook>), With<ZooCamera>>,
    intents: Query<&CameraIntent, With<ZooCamera>>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    let (Some(definitions), Ok((entity, mode, camera, allowed))) =
        (definitions.get(&assets), active_modes.single())
    else {
        action_requests.clear();
        return;
    };
    let Some(policy) = find_authored_immersive_mode_policy(definitions, mode.mode)
        .and_then(|policy| policy.first_person_movement.as_ref())
    else {
        action_requests.clear();
        return;
    };
    let commands = intents
        .get(camera.0)
        .map(|intent| intent.first_person_commands)
        .unwrap_or_default();
    apply_allowed_look_rotation_and_cancellation_to_first_person_view(
        &mut action_requests,
        &control_axes,
        time.delta_secs(),
        camera.0,
        allowed,
        entity,
        active_input.source,
        &mut view_angles,
        &camera_tuning,
        &mut exit_requests,
        Some((
            commands,
            [policy.turn_speed_out_rps, policy.keyboard_rotate_speed_rps],
        )),
    );
}

pub(super) fn update_subject_relative_first_person_camera_transform(
    active_modes: Query<(&ActiveImmersiveMode, &ImmersiveSubject, &ImmersiveCamera)>,
    view_angles: Query<&FirstPersonViewAngles>,
    assembly_views: Query<(
        &GlobalTransform,
        &super::immersive_mode_state_types::FossilAssemblyView,
    )>,
    // A ridden vehicle drives itself, so it has no `ControlledEntity`.
    controlled_subjects: Query<
        (&GlobalTransform, Has<openzt2_interface_state::camera::camera_runtime_state_types::FirstPersonLocomotionTuning>),
        Or<(
            With<ControlledEntity>,
            With<openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle>,
        )>,
    >,
    mut zoo_cameras: Query<
        (&mut Transform, &CameraMode),
        (With<ZooCamera>, Without<StandaloneCameraSubject>),
    >,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
) {
    let Ok((active_mode, subject, camera)) = active_modes.single() else {
        return;
    };
    if active_mode.mode == ImmersiveMode::FossilAssembly {
        if let (Ok((table, view)), Ok((mut pose, CameraMode::FirstPerson))) =
            (assembly_views.get(subject.0), zoo_cameras.get_mut(camera.0))
        {
            pose.set_if_neq(table.compute_transform() * view.table_relative_camera);
        }
        return;
    }
    if !matches!(active_mode.mode, ImmersiveMode::FirstPerson) {
        return;
    }
    let subject_entity = subject.0;
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let (Ok((subject_transform, walker)), Ok((mut camera_transform, CameraMode::FirstPerson))) = (
        controlled_subjects.get(subject_entity),
        zoo_cameras.get_mut(camera.0),
    ) else {
        return;
    };
    let Some(camera_policy) =
        find_authored_immersive_mode_policy(world_definitions, active_mode.mode)
    else {
        return;
    };
    let Some(camera_tuning) = world_definitions.find_camera(AssetId(camera_policy.camera.0)) else {
        return;
    };
    let camera_offset = Vec3::from_array(camera_tuning.offset_m.map(|value| value));
    let mut next_camera_transform =
        subject_transform.compute_transform() * Transform::from_translation(camera_offset);
    if active_mode.mode == ImmersiveMode::FirstPerson {
        if let Ok(view_angles) = view_angles.single() {
            next_camera_transform.rotation *=
                Quat::from_rotation_y(view_angles.yaw) * Quat::from_rotation_x(view_angles.pitch);
        }
    }
    if walker && active_mode.mode == ImmersiveMode::FirstPerson {
        next_camera_transform.translation.y = camera_transform.translation.y;
    }
    camera_transform.set_if_neq(next_camera_transform);
}
