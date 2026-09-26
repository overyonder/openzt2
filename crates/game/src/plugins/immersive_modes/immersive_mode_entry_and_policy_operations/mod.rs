use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::immersive_mode_policy::{ImmersiveModeKind, ImmersiveModePolicy},
    AssetId,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::camera::camera_control_message_types::SetCameraMode;
use crate::plugins::camera::camera_runtime_state_types::CameraMode;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::locomotion::locomotion_types::DirectLocomotion;
use crate::plugins::photos::photo_capture_types::PhotoMode;

use super::{
    immersive_mode_control_types::FirstPersonViewAngles,
    immersive_mode_message_types::{ImmersiveModeEntered, ModeEntryFailure},
    immersive_mode_policy_types::{
        AllowedInteractionActions, HideHudWhileActive, InteractionCursor, InteractionPrefab,
        PauseSimulationWhileActive,
    },
    immersive_mode_state_types::{
        ActiveImmersiveMode, ControlledEntity, EquippedInteractionTool, ImmersiveMode,
        PendingImmersiveEntry,
    },
};

pub(in crate::plugins::immersive_modes) fn find_pending_entry_for_immersive_mode<'a>(
    mode: ImmersiveMode,
    pending: &'a Query<(Entity, &PendingImmersiveEntry)>,
) -> Option<(Entity, &'a PendingImmersiveEntry)> {
    pending.iter().find(|(_, intent)| intent.mode == mode)
}

pub(in crate::plugins::immersive_modes) fn require_subject_from_pending_immersive_mode_entry(
    controller: Entity,
    intent: &PendingImmersiveEntry,
    commands: &mut Commands,
) -> Option<Entity> {
    intent.subject.or_else(|| {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::MissingSubject,
            commands,
        );
        None
    })
}

pub(in crate::plugins::immersive_modes) fn record_pending_immersive_mode_entry_failure(
    controller: Entity,
    intent: &PendingImmersiveEntry,
    failure: ModeEntryFailure,
    commands: &mut Commands,
) {
    commands
        .entity(controller)
        .insert(copy_pending_immersive_mode_entry_with_failure(
            intent, failure,
        ));
}

pub(crate) fn copy_pending_immersive_mode_entry_with_failure(
    intent: &PendingImmersiveEntry,
    failure: ModeEntryFailure,
) -> PendingImmersiveEntry {
    PendingImmersiveEntry {
        mode: intent.mode,
        subject: intent.subject,
        failure: Some(failure),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::immersive_modes) fn commit_validated_immersive_mode_entry<M: Component>(
    controller: Entity,
    subject: Entity,
    intent: &PendingImmersiveEntry,
    cameras: &Query<Entity, With<ZooCamera>>,
    marker: M,
    tool: Option<AssetId>,
    camera_mode: CameraMode,
    camera_definition: AssetId,
    directly_controlled: bool,
    restore_camera_on_exit: bool,
    commands: &mut Commands,
    camera_requests: &mut MessageWriter<SetCameraMode>,
    entered: &mut MessageWriter<ImmersiveModeEntered>,
) -> bool {
    let Ok(camera) = cameras.single() else {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::RuleDenied,
            commands,
        );
        return false;
    };
    let mut controller_commands = commands.entity(controller);
    controller_commands
        .insert(marker)
        .remove::<PendingImmersiveEntry>();
    if let Some(definition) = tool {
        controller_commands.insert(EquippedInteractionTool { policy: definition });
    }
    if directly_controlled {
        commands.entity(subject).insert((
            ControlledEntity { controller },
            DirectLocomotion {
                local_axes: Vec2::ZERO,
            },
        ));
    }
    if intent.mode == ImmersiveMode::Photo {
        commands.entity(camera).insert(PhotoMode);
    }
    commands.entity(controller).insert(ActiveImmersiveMode {
        mode: intent.mode,
        subject: intent.subject,
        camera,
        restore_camera_on_exit,
    });
    camera_requests.write(SetCameraMode {
        mode: camera_mode,
        definition: camera_definition,
        transition_seconds: None,
    });
    entered.write(ImmersiveModeEntered {
        mode: intent.mode,
        subject: intent.subject,
    });
    true
}

#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::immersive_modes) fn commit_authored_interaction_tool_immersive_mode_entry<
    M: Component,
>(
    mode: ImmersiveMode,
    controller: Entity,
    subject: Entity,
    intent: &PendingImmersiveEntry,
    defs: WorldDefinitionsView<'_>,
    cameras: &Query<Entity, With<ZooCamera>>,
    marker: M,
    commands: &mut Commands,
    camera_requests: &mut MessageWriter<SetCameraMode>,
    entered: &mut MessageWriter<ImmersiveModeEntered>,
) -> bool {
    let Some(policy) = find_authored_immersive_mode_policy(defs, mode) else {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::MissingTool,
            commands,
        );
        return false;
    };
    if defs.find_camera(AssetId(policy.camera.0)).is_none() {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::MissingTool,
            commands,
        );
        return false;
    }
    let camera_mode = match mode {
        ImmersiveMode::Photo => CameraMode::Free,
        ImmersiveMode::ShowEdit => CameraMode::Follow(subject),
        _ => CameraMode::FirstPerson(subject),
    };
    if !commit_validated_immersive_mode_entry(
        controller,
        subject,
        intent,
        cameras,
        marker,
        Some(AssetId(policy.id.0)),
        camera_mode,
        AssetId(policy.camera.0),
        policy.lock_subject,
        policy.restore_camera_on_exit,
        commands,
        camera_requests,
        entered,
    ) {
        return false;
    }
    attach_authored_immersive_mode_consumer_policies(defs, mode, controller, commands);
    true
}

pub(in crate::plugins::immersive_modes) fn resolve_required_authored_interaction_policy(
    mode: ImmersiveMode,
    controller: Entity,
    intent: &PendingImmersiveEntry,
    defs: WorldDefinitionsView<'_>,
    commands: &mut Commands,
) -> Option<(AssetId, AssetId, bool, bool)> {
    let Some(policy) = find_authored_immersive_mode_policy(defs, mode) else {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::MissingTool,
            commands,
        );
        return None;
    };
    let camera = AssetId(policy.camera.0);
    if defs.find_camera(camera).is_none() {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::MissingTool,
            commands,
        );
        return None;
    }
    Some((
        AssetId(policy.id.0),
        camera,
        policy.lock_subject,
        policy.restore_camera_on_exit,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::immersive_modes) fn enter_immersive_mode_for_valid_subject_with_authored_tool<
    M: Component,
    D: bevy::ecs::query::QueryData,
>(
    mode: ImmersiveMode,
    pending: &Query<(Entity, &PendingImmersiveEntry)>,
    subjects: &Query<D>,
    definitions: Option<WorldDefinitionsView<'_>>,
    cameras: &Query<Entity, With<ZooCamera>>,
    marker: M,
    commands: &mut Commands,
    camera_requests: &mut MessageWriter<SetCameraMode>,
    entered: &mut MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller, intent)) = find_pending_entry_for_immersive_mode(mode, pending) else {
        return;
    };
    let Some(subject) =
        require_subject_from_pending_immersive_mode_entry(controller, intent, commands)
    else {
        return;
    };
    if subjects.get(subject).is_err() {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::WrongSubject,
            commands,
        );
        return;
    }
    let Some(definitions) = definitions else {
        record_pending_immersive_mode_entry_failure(
            controller,
            intent,
            ModeEntryFailure::MissingTool,
            commands,
        );
        return;
    };
    let _ = commit_authored_interaction_tool_immersive_mode_entry(
        mode,
        controller,
        subject,
        intent,
        definitions,
        cameras,
        marker,
        commands,
        camera_requests,
        entered,
    );
}

pub(in crate::plugins::immersive_modes) fn calculate_initial_first_person_view_angles(
    definitions: WorldDefinitionsView<'_>,
    camera_definition: AssetId,
) -> FirstPersonViewAngles {
    definitions.find_camera(camera_definition).map_or_else(
        FirstPersonViewAngles::default,
        |camera| FirstPersonViewAngles {
            yaw: (camera.yaw_radians[0] + camera.yaw_radians[1]) * 0.5,
            pitch: camera.initial_pitch_radians,
        },
    )
}

pub(super) fn find_authored_immersive_mode_policy<'a>(
    definitions: WorldDefinitionsView<'a>,
    mode: ImmersiveMode,
) -> Option<&'a ImmersiveModePolicy> {
    definitions
        .immersive_mode_policies()
        .find(|policy| authored_mode_kind_matches_immersive_mode(&policy.mode, mode))
}

pub(in crate::plugins::immersive_modes) fn attach_authored_immersive_mode_consumer_policies(
    definitions: WorldDefinitionsView<'_>,
    mode: ImmersiveMode,
    controller: Entity,
    commands: &mut Commands,
) {
    let Some(policy) = find_authored_immersive_mode_policy(definitions, mode) else {
        return;
    };
    let mut entity = commands.entity(controller);
    entity.insert(AllowedInteractionActions {
        allowed_actions: policy.allowed_actions,
    });
    if let Some(image) = definitions.texture_image(AssetId(policy.interaction_cursor.0)) {
        entity.insert(InteractionCursor { image });
    }
    let definition = AssetId(policy.interaction_prefab.0);
    if let Some(handle) = definitions.scene(definition) {
        entity.insert(InteractionPrefab { definition, handle });
    }
    if policy.pause_simulation {
        entity.insert(PauseSimulationWhileActive);
    }
    if policy.hide_hud {
        entity.insert(HideHudWhileActive);
    }
}

fn authored_mode_kind_matches_immersive_mode(
    authored_mode_kind: &ImmersiveModeKind,
    immersive_mode: ImmersiveMode,
) -> bool {
    matches!(
        (authored_mode_kind, immersive_mode),
        (ImmersiveModeKind::GuestView, ImmersiveMode::GuestView)
            | (ImmersiveModeKind::FirstPerson, ImmersiveMode::FirstPerson)
            | (ImmersiveModeKind::FossilSearch, ImmersiveMode::FossilSearch)
            | (
                ImmersiveModeKind::FossilAssembly,
                ImmersiveMode::FossilAssembly
            )
            | (ImmersiveModeKind::Photo, ImmersiveMode::Photo)
            | (ImmersiveModeKind::ShowEdit, ImmersiveMode::ShowEdit)
            | (ImmersiveModeKind::SuperStaff, ImmersiveMode::SuperStaff)
    )
}
