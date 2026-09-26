//! Enter the ordinary first-person child by walking a super-staff avatar.

use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::camera::camera_control_message_types::SetCameraMode;
use crate::plugins::camera::camera_runtime_state_types::{CameraMode, ZooCamera};
use crate::plugins::locomotion::locomotion_types::{NavAgent, NavFlags};

use super::{
    immersive_mode_control_types::FirstPersonControl,
    immersive_mode_entry_and_policy_operations::{
        attach_authored_immersive_mode_consumer_policies,
        calculate_initial_first_person_view_angles, commit_validated_immersive_mode_entry,
        find_pending_entry_for_immersive_mode, record_pending_immersive_mode_entry_failure,
        resolve_required_authored_interaction_policy,
    },
    immersive_mode_message_types::{ImmersiveModeEntered, ModeEntryFailure},
    immersive_mode_state_types::{ImmersiveMode, PendingImmersiveEntry},
    super_staff_avatar_start_position_search::SuperStaffAvatarStartSurface,
};

pub(super) fn enter_pending_super_staff_mode(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    (definitions, active_definitions): (Res<Assets<WorldDefinitionAsset>>, Res<WorldDefinitions>),
    cameras: Query<Entity, With<ZooCamera>>,
    camera_transforms: Query<&Transform, With<ZooCamera>>,
    start_surface: SuperStaffAvatarStartSurface,
    mut commands: Commands,
    mut camera_requests: MessageWriter<SetCameraMode>,
    mut entered: MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller, pending)) =
        find_pending_entry_for_immersive_mode(ImmersiveMode::SuperStaff, &pending_entries)
    else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        record_pending_immersive_mode_entry_failure(
            controller,
            pending,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    };
    let Some((interaction_tool, camera_definition, _, restore_camera_on_exit)) =
        resolve_required_authored_interaction_policy(
            ImmersiveMode::SuperStaff,
            controller,
            pending,
            definitions,
            &mut commands,
        )
    else {
        return;
    };
    let (Some(walking_camera), Ok(overhead_camera)) = (
        definitions.find_camera(camera_definition),
        camera_transforms.single(),
    ) else {
        record_pending_immersive_mode_entry_failure(
            controller,
            pending,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    };
    let avatar_locomotion = NavAgent {
        radius_m: f32::from(walking_camera.collision_radius_cm) * 0.01,
        max_speed_mps: walking_camera.pan_speed_mps,
        acceleration_mps2: walking_camera.ground_acceleration_mps2,
        capabilities: NavFlags::STAFF,
    };
    if !avatar_locomotion.is_valid() {
        record_pending_immersive_mode_entry_failure(
            controller,
            pending,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    }

    // The player starts facing the overhead view's heading, with the
    // first-person camera's own initial pitch.
    let (heading_yaw, _, _) = overhead_camera.rotation.to_euler(EulerRot::YXZ);
    let avatar_transform = Transform::from_translation(
        start_surface.find_avatar_start_position(overhead_camera.translation, heading_yaw),
    )
    .with_rotation(Quat::from_rotation_y(heading_yaw));
    // The camera reads the avatar's global pose in this frame, before
    // transform propagation has run for the new entity.
    let avatar = commands
        .spawn((
            Name::new("super staff avatar"),
            avatar_transform,
            GlobalTransform::from(avatar_transform),
            avatar_locomotion,
        ))
        .id();
    let entry = PendingImmersiveEntry {
        subject: Some(avatar),
        ..*pending
    };
    if !commit_validated_immersive_mode_entry(
        controller,
        avatar,
        &entry,
        &cameras,
        FirstPersonControl,
        Some(interaction_tool),
        CameraMode::FirstPerson(avatar),
        camera_definition,
        true,
        restore_camera_on_exit,
        &mut commands,
        &mut camera_requests,
        &mut entered,
    ) {
        commands.entity(avatar).despawn();
        return;
    }
    commands
        .entity(controller)
        .insert(calculate_initial_first_person_view_angles(
            definitions,
            camera_definition,
        ));
    attach_authored_immersive_mode_consumer_policies(
        definitions,
        ImmersiveMode::SuperStaff,
        controller,
        &mut commands,
    );
}
