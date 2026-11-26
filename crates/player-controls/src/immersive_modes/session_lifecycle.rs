use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::camera::camera_control_message_types::RestoreCameraMode;
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;

use super::immersive_mode_message_types::{ExitImmersiveMode, ImmersiveModeExited, ModeExitReason};
use super::immersive_mode_state_types::{
    ActiveImmersiveMode, ControlledEntity, EquippedInteractionTool, FirstPersonTraining,
    FirstPersonTrainingSessions, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
};
use openzt2_zoo_state::{animal_health::types::Dead, animal_lifecycle::types::Animal};

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
    commands
        .entity(removed.entity)
        .try_remove::<FirstPersonTraining>();
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

fn end_training_for_dead_subject(
    death: On<Add<Dead>>,
    subjects: Query<&FirstPersonTrainingSessions>,
    mut commands: Commands,
) {
    end_training_for_subject(death.entity, &subjects, &mut commands);
}

fn end_training_for_removed_animal(
    removal: On<Remove<Animal>>,
    subjects: Query<&FirstPersonTrainingSessions>,
    mut commands: Commands,
) {
    end_training_for_subject(removal.entity, &subjects, &mut commands);
}

fn end_training_for_subject(
    subject: Entity,
    subjects: &Query<&FirstPersonTrainingSessions>,
    commands: &mut Commands,
) {
    if let Ok(sessions) = subjects.get(subject) {
        for session in sessions.iter() {
            commands.entity(session).try_remove::<FirstPersonTraining>();
        }
    }
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
    commands.entity(removed.entity).try_remove::<(
        DirectLocomotion,
        openzt2_interface_state::camera::camera_runtime_state_types::FirstPersonLocomotionTuning,
    )>();
}

pub(super) fn register(app: &mut App) {
    app.add_observer(release_session)
        .add_observer(release_direct_locomotion)
        .add_observer(end_training_for_dead_subject)
        .add_observer(end_training_for_removed_animal)
        .add_observer(exit_when_link_breaks::<ImmersiveSubject>)
        .add_observer(exit_when_link_breaks::<ImmersiveCamera>)
        .add_observer(exit_when_link_breaks::<EquippedInteractionTool>);
}
