//! Enter the ordinary first-person child by walking a super-staff avatar.

use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::CameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::immersive_mode_entry_and_policy_operations::attach_authored_immersive_mode_consumer_policies;
use super::immersive_mode_entry_and_policy_operations::calculate_initial_first_person_view_angles;
use super::immersive_mode_entry_and_policy_operations::commit_validated_immersive_mode_entry;
use super::immersive_mode_entry_and_policy_operations::find_pending_entry_for_immersive_mode;
use super::immersive_mode_entry_and_policy_operations::record_pending_immersive_mode_entry_failure;
use super::immersive_mode_entry_and_policy_operations::resolve_required_authored_interaction_policy;
use super::immersive_mode_message_types::ImmersiveModeEntered;
use super::immersive_mode_message_types::ModeEntryFailure;
use super::immersive_mode_state_types::ImmersiveMode;
use super::super_staff_avatar_start_position_search::SuperStaffAvatarStartSurface;
use crate::immersive_modes::immersive_control_execution_types::FirstPersonControl;
use crate::immersive_modes::immersive_control_execution_types::PendingImmersiveEntry;

pub(super) fn enter_pending_super_staff_mode(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    (definitions, active_definitions): (Res<Assets<WorldDefinitionAsset>>, Res<WorldDefinitions>),
    cameras: Query<Entity, With<ZooCamera>>,
    camera_transforms: Query<(&Transform, &WorldMember), With<ZooCamera>>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
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
    let (Some(walking_camera), Ok((overhead_camera, world_member))) = (
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
    let Ok(bounds) = world_bounds.get(world_member.root) else {
        record_pending_immersive_mode_entry_failure(
            controller,
            pending,
            ModeEntryFailure::RuleDenied,
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
    let avatar_transform = Transform::from_translation(start_surface.find_avatar_start_position(
        overhead_camera.translation,
        heading_yaw,
        bounds,
    ))
    .with_rotation(Quat::from_rotation_y(heading_yaw));
    // The camera reads the avatar's global pose in this frame, before
    // transform propagation has run for the new entity.
    let avatar = commands
        .spawn((
            Name::new("super staff avatar"),
            avatar_transform,
            GlobalTransform::from(avatar_transform),
            avatar_locomotion,
            *world_member,
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
