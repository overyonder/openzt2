use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::input::input_types::ActionRequest;
use crate::plugins::camera::camera_runtime_state_types::CameraMode;
use crate::plugins::camera::camera_runtime_state_types::CameraMouseLook;
use crate::plugins::camera::camera_runtime_state_types::CameraTuning;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::locomotion::locomotion_types::DirectLocomotion;

use super::{
    first_person_mode_control_operations::{
        apply_allowed_look_rotation_and_cancellation_to_first_person_view,
        apply_allowed_navigation_axes_to_directly_controlled_subject,
    },
    immersive_mode_control_types::{FirstPersonControl, FirstPersonViewAngles, GuestViewControl},
    immersive_mode_entry_and_policy_operations::find_authored_immersive_mode_policy,
    immersive_mode_message_types::ExitImmersiveMode,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::{ActiveImmersiveMode, ControlledEntity, ImmersiveMode},
};

pub(super) fn apply_guest_view_look_and_navigation_controls(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    control_axes: Res<crate::plugins::input::input_types::DeviceControlAxes>,
    active_input: Res<crate::plugins::input::input_types::ActiveInputDevice>,
    time: Res<Time<Real>>,
    active_modes: Query<(
        Entity,
        &ActiveImmersiveMode,
        Option<&AllowedInteractionActions>,
    )>,
    mut view_angles: Query<&mut FirstPersonViewAngles, With<GuestViewControl>>,
    camera_tuning: Query<(&CameraTuning, Has<CameraMouseLook>), With<ZooCamera>>,
    mut controlled_subjects: Query<(&ControlledEntity, &mut DirectLocomotion)>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    let Ok((controller_entity, active_mode, allowed_actions)) = active_modes.single() else {
        return;
    };
    if modal_input.0.is_some() {
        action_requests.clear();
        apply_allowed_navigation_axes_to_directly_controlled_subject(
            &crate::plugins::input::input_types::DeviceControlAxes::default(),
            active_mode,
            allowed_actions,
            controller_entity,
            0.0,
            &mut controlled_subjects,
        );
        return;
    }
    apply_allowed_look_rotation_and_cancellation_to_first_person_view(
        &mut action_requests,
        &control_axes,
        time.delta_secs(),
        active_mode,
        allowed_actions,
        controller_entity,
        active_input.source,
        &mut view_angles,
        &camera_tuning,
        &mut exit_requests,
    );
    apply_allowed_navigation_axes_to_directly_controlled_subject(
        &control_axes,
        active_mode,
        allowed_actions,
        controller_entity,
        view_angles.single().map_or(0.0, |angles| angles.yaw),
        &mut controlled_subjects,
    );
}

pub(super) fn apply_first_person_look_and_navigation_controls(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    control_axes: Res<crate::plugins::input::input_types::DeviceControlAxes>,
    active_input: Res<crate::plugins::input::input_types::ActiveInputDevice>,
    time: Res<Time<Real>>,
    active_modes: Query<(
        Entity,
        &ActiveImmersiveMode,
        Option<&AllowedInteractionActions>,
    )>,
    mut view_angles: Query<&mut FirstPersonViewAngles, With<FirstPersonControl>>,
    camera_tuning: Query<(&CameraTuning, Has<CameraMouseLook>), With<ZooCamera>>,
    mut controlled_subjects: Query<(&ControlledEntity, &mut DirectLocomotion)>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    let Ok((controller_entity, active_mode, allowed_actions)) = active_modes.single() else {
        return;
    };
    if modal_input.0.is_some() {
        action_requests.clear();
        apply_allowed_navigation_axes_to_directly_controlled_subject(
            &crate::plugins::input::input_types::DeviceControlAxes::default(),
            active_mode,
            allowed_actions,
            controller_entity,
            0.0,
            &mut controlled_subjects,
        );
        return;
    }
    apply_allowed_look_rotation_and_cancellation_to_first_person_view(
        &mut action_requests,
        &control_axes,
        time.delta_secs(),
        active_mode,
        allowed_actions,
        controller_entity,
        active_input.source,
        &mut view_angles,
        &camera_tuning,
        &mut exit_requests,
    );
    apply_allowed_navigation_axes_to_directly_controlled_subject(
        &control_axes,
        active_mode,
        allowed_actions,
        controller_entity,
        view_angles.single().map_or(0.0, |angles| angles.yaw),
        &mut controlled_subjects,
    );
}

pub(super) fn update_subject_relative_first_person_camera_transform(
    active_modes: Query<&ActiveImmersiveMode>,
    view_angles: Query<&FirstPersonViewAngles>,
    controlled_subjects: Query<&GlobalTransform, With<ControlledEntity>>,
    mut zoo_cameras: Query<(&mut Transform, &CameraMode), With<ZooCamera>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
) {
    let Ok(active_mode) = active_modes.single() else {
        return;
    };
    if !matches!(
        active_mode.mode,
        ImmersiveMode::GuestView
            | ImmersiveMode::FirstPerson
            | ImmersiveMode::SuperStaff
            | ImmersiveMode::FossilSearch
            | ImmersiveMode::FossilAssembly
    ) {
        return;
    }
    let (Some(subject_entity), Some(world_definitions)) = (
        active_mode.subject,
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    let (
        Ok(subject_transform),
        Ok((mut camera_transform, CameraMode::FirstPerson(camera_subject_entity))),
    ) = (
        controlled_subjects.get(subject_entity),
        zoo_cameras.get_mut(active_mode.camera),
    )
    else {
        return;
    };
    if *camera_subject_entity != subject_entity {
        return;
    }
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
    if matches!(
        active_mode.mode,
        ImmersiveMode::GuestView | ImmersiveMode::FirstPerson | ImmersiveMode::SuperStaff
    ) {
        if let Ok(view_angles) = view_angles.single() {
            next_camera_transform.rotation *=
                Quat::from_rotation_y(view_angles.yaw) * Quat::from_rotation_x(view_angles.pitch);
        }
    }
    *camera_transform = next_camera_transform;
}
