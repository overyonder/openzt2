use bevy::prelude::*;

use crate::plugins::{
    camera::camera_control_message_types::RestoreCameraMode,
    locomotion::locomotion_types::DirectLocomotion, photos::photo_capture_types::PhotoMode,
};

use super::{
    immersive_mode_control_types::{
        FirstPersonControl, FirstPersonViewAngles, FossilAssemblyControl, FossilSearchControl,
        GuestViewControl, GuestViewTileCount, PhotoControl, ShowEditControl,
    },
    immersive_mode_message_types::{ExitImmersiveMode, ImmersiveModeExited, ModeExitReason},
    immersive_mode_policy_types::{
        AllowedInteractionActions, HideHudWhileActive, InteractionCursor, InteractionPrefab,
        PauseSimulationWhileActive,
    },
    immersive_mode_state_types::{
        ActiveImmersiveMode, ControlledEntity, EquippedInteractionTool, ImmersiveMode,
        PendingImmersiveEntry,
    },
};

pub(super) fn request_immersive_mode_exit_when_subject_or_tool_is_removed(
    active_immersive_modes: Query<(Entity, &ActiveImmersiveMode)>,
    existing_entities: Query<()>,
    mut removed_controlled_entities: RemovedComponents<ControlledEntity>,
    mut removed_interaction_tools: RemovedComponents<EquippedInteractionTool>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    let Ok((controller_entity, active_mode)) = active_immersive_modes.single() else {
        return;
    };
    let participant_was_removed = active_mode
        .subject
        .is_some_and(|subject_entity| existing_entities.get(subject_entity).is_err())
        || removed_controlled_entities
            .read()
            .any(|entity| Some(entity) == active_mode.subject)
        || removed_interaction_tools
            .read()
            .any(|entity| entity == controller_entity);
    if participant_was_removed {
        exit_requests.write(ExitImmersiveMode {
            controller: controller_entity,
            reason: ModeExitReason::SubjectRemoved,
        });
    }
}

pub(super) fn apply_requested_immersive_mode_exit_and_remove_owned_state(
    mut exit_requests: MessageReader<ExitImmersiveMode>,
    active_immersive_modes: Query<(Entity, &ActiveImmersiveMode)>,
    mut commands: Commands,
    mut restore_camera_requests: MessageWriter<RestoreCameraMode>,
    mut exited_messages: MessageWriter<ImmersiveModeExited>,
) {
    let Ok((controller_entity, active_mode)) = active_immersive_modes.single() else {
        return;
    };
    let Some(exit_request) = exit_requests
        .read()
        .find(|exit_request| exit_request.controller == controller_entity)
    else {
        return;
    };
    remove_active_immersive_mode_state_and_restore_camera(
        active_mode,
        controller_entity,
        exit_request.reason,
        &mut commands,
        &mut restore_camera_requests,
        &mut exited_messages,
    );
}

pub(super) fn remove_active_immersive_mode_state_when_leaving_gameplay(
    active_immersive_modes: Query<(Entity, &ActiveImmersiveMode)>,
    mut commands: Commands,
    mut restore_camera_requests: MessageWriter<RestoreCameraMode>,
    mut exited_messages: MessageWriter<ImmersiveModeExited>,
) {
    let Ok((controller_entity, active_mode)) = active_immersive_modes.single() else {
        return;
    };
    remove_active_immersive_mode_state_and_restore_camera(
        active_mode,
        controller_entity,
        ModeExitReason::GamePhaseChanged,
        &mut commands,
        &mut restore_camera_requests,
        &mut exited_messages,
    );
}

fn remove_active_immersive_mode_state_and_restore_camera(
    active_mode: &ActiveImmersiveMode,
    controller_entity: Entity,
    exit_reason: ModeExitReason,
    commands: &mut Commands,
    restore_camera_requests: &mut MessageWriter<RestoreCameraMode>,
    exited_messages: &mut MessageWriter<ImmersiveModeExited>,
) {
    if let Some(subject_entity) = active_mode.subject {
        // The super-staff avatar exists only while the player walks as it.
        if active_mode.mode == ImmersiveMode::SuperStaff {
            if let Ok(mut avatar) = commands.get_entity(subject_entity) {
                avatar.despawn();
            }
        } else {
            commands
                .entity(subject_entity)
                .remove::<(ControlledEntity, DirectLocomotion)>();
        }
    }
    commands.entity(controller_entity).remove::<(
        ActiveImmersiveMode,
        PendingImmersiveEntry,
        EquippedInteractionTool,
        AllowedInteractionActions,
        InteractionCursor,
        InteractionPrefab,
        PauseSimulationWhileActive,
        HideHudWhileActive,
    )>();
    commands.entity(controller_entity).remove::<(
        GuestViewControl,
        GuestViewTileCount,
        FirstPersonControl,
        FirstPersonViewAngles,
        FossilSearchControl,
        FossilAssemblyControl,
    )>();
    commands
        .entity(controller_entity)
        .remove::<(PhotoControl, ShowEditControl)>();
    commands.entity(active_mode.camera).remove::<PhotoMode>();
    if active_mode.restore_camera_on_exit {
        restore_camera_requests.write(RestoreCameraMode {
            transition_seconds: None,
        });
    }
    exited_messages.write(ImmersiveModeExited {
        mode: active_mode.mode,
        reason: exit_reason,
    });
}
