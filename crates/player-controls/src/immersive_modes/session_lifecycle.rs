use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::camera::camera_control_message_types::RestoreCameraMode;
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;

use super::immersive_mode_message_types::{ExitImmersiveMode, ImmersiveModeExited, ModeExitReason};
use super::immersive_mode_state_types::{
    ActiveImmersiveMode, ControlledEntity, EquippedInteractionTool, ImmersiveCamera, ImmersiveMode,
    ImmersiveSubject,
};

#[derive(Component)]
pub(super) struct SessionExit(ModeExitReason);

pub(super) fn exit_requested_session(
    mut requests: MessageReader<ExitImmersiveMode>,
    active: Query<Entity, With<ActiveImmersiveMode>>,
    mut commands: Commands,
) {
    let session = active.single().ok();
    if let Some(request) = requests
        .read()
        .find(|request| Some(request.controller) == session)
    {
        commands
            .entity(request.controller)
            .insert(SessionExit(request.reason))
            .despawn();
    }
}

pub(super) fn release_session(
    removed: On<Remove<ActiveImmersiveMode>>,
    sessions: Query<(
        &ActiveImmersiveMode,
        Option<&ImmersiveCamera>,
        Option<&SessionExit>,
    )>,
    phase: Res<State<GamePhase>>,
    mut commands: Commands,
    mut restore: MessageWriter<RestoreCameraMode>,
    mut exited: MessageWriter<ImmersiveModeExited>,
) {
    let Ok((active, camera, reason)) = sessions.get(removed.entity) else {
        return;
    };
    let reason = reason.map_or_else(
        || {
            if *phase.get() == GamePhase::InGame {
                ModeExitReason::SubjectRemoved
            } else {
                ModeExitReason::GamePhaseChanged
            }
        },
        |reason| reason.0,
    );
    if active.mode == ImmersiveMode::Photo {
        if let Some(camera) = camera {
            commands.entity(camera.0).try_remove::<PhotoMode>();
        }
    }
    if active.restore_camera_on_exit {
        restore.write(RestoreCameraMode {
            transition_seconds: None,
        });
    }
    exited.write(ImmersiveModeExited {
        mode: active.mode,
        reason,
    });
}

pub(super) fn exit_when_link_breaks<T: Component>(
    removed: On<Remove<T>>,
    active: Query<(), With<ActiveImmersiveMode>>,
    mut exit: MessageWriter<ExitImmersiveMode>,
) {
    if active.contains(removed.entity) {
        exit.write(ExitImmersiveMode {
            controller: removed.entity,
            reason: ModeExitReason::SubjectRemoved,
        });
    }
}

pub(super) fn release_direct_locomotion(
    removed: On<Remove<ControlledEntity>>,
    subjects: Query<&ControlledEntity>,
    active: Query<(), With<ActiveImmersiveMode>>,
    mut commands: Commands,
    mut exit: MessageWriter<ExitImmersiveMode>,
) {
    if let Ok(subject) = subjects.get(removed.entity) {
        if active.contains(subject.controller) {
            exit.write(ExitImmersiveMode {
                controller: subject.controller,
                reason: ModeExitReason::SubjectRemoved,
            });
        }
    }
    commands
        .entity(removed.entity)
        .try_remove::<DirectLocomotion>();
}

pub(super) fn register(app: &mut App) {
    app.add_observer(release_session)
        .add_observer(release_direct_locomotion)
        .add_observer(exit_when_link_breaks::<ImmersiveSubject>)
        .add_observer(exit_when_link_breaks::<ImmersiveCamera>)
        .add_observer(exit_when_link_breaks::<EquippedInteractionTool>);
}
