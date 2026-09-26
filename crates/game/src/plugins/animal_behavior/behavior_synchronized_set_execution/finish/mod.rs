use super::super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use super::{SynchronizedBehaviorOwner, SynchronizedBehaviorParticipant};
use crate::plugins::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackState,
    },
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, BehaviorTaskExecutionState, BehaviorTaskFailed,
        BehaviorTaskFinished, PendingBehaviorAnimationClipCompletion,
        PendingBehaviorDockingCompletion, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{Destination, Docking},
    simulation_time::simulation_clock_types::ZooClock,
};
use bevy::{ecs::system::SystemParam, prelude::*};

#[derive(SystemParam)]
pub(super) struct SynchronizedBehaviorOutcomes<'w, 's> {
    finished: MessageReader<'w, 's, BehaviorTaskFinished>,
    failed: MessageReader<'w, 's, BehaviorTaskFailed>,
    controllers: Query<
        'w,
        's,
        (
            &'static AnimationPresentationOwner,
            &'static mut AnimationPlaybackController,
        ),
    >,
}

pub(super) fn finish_or_cancel_synchronized_behavior_sets(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut outcomes: SynchronizedBehaviorOutcomes,
    mut owners: Query<(
        Entity,
        &mut SynchronizedBehaviorOwner,
        Option<&mut BehaviorTaskExecutionState>,
        Option<&PendingBehaviorTaskFailure>,
    )>,
    participants: Query<(Entity, &SynchronizedBehaviorParticipant)>,
) {
    for event in outcomes.finished.read() {
        if let Ok((_, participant)) = participants.get(event.actor) {
            if let Ok((_, mut owner, _, _)) = owners.get_mut(participant.owner) {
                if owner.target_execution_id == event.execution_id {
                    owner.target_outcome = Some(true);
                }
            }
        }
    }
    for event in outcomes.failed.read() {
        if let Ok((_, participant)) = participants.get(event.actor) {
            if let Ok((_, mut owner, _, _)) = owners.get_mut(participant.owner) {
                if owner.target_execution_id == event.execution_id {
                    owner.target_outcome = Some(false);
                }
            }
        }
    }
    for (actor, owner, mut task, failure) in &mut owners {
        let owns_task = task.as_ref().is_some_and(|task| {
            task.execution_id == owner.execution_id
                && task.target() == Some(owner.target)
                && task.is_at_or_nested_under(&owner.origin, owner.depth)
        });
        let owns_partner = participants
            .get(owner.target)
            .is_ok_and(|(_, participant)| {
                participant.owner == actor && participant.execution_id == owner.target_execution_id
            });
        let subject_finished = owns_task
            && task
                .as_ref()
                .is_some_and(|task| task.stack.len() == owner.depth);
        let failed = failure.is_some() || !owns_partner || owner.target_outcome == Some(false);
        if owns_task && !failed && !(subject_finished && owner.target_outcome == Some(true)) {
            continue;
        }
        if owns_task {
            if let Some(task) = task.as_deref_mut() {
                task.restore_suspended_instruction(&owner.origin, owner.depth, clock.tick);
                if failed {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                } else {
                    advance_behavior_task_to_next_action(task, clock.tick);
                }
            }
        }
        if owns_partner {
            commands.entity(owner.target).remove::<(
                SynchronizedBehaviorParticipant,
                BehaviorTaskExecutionState,
                PendingBehaviorTaskFailure,
                PendingBehaviorAnimationClipCompletion,
                PendingBehaviorDockingCompletion,
                crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
                Destination,
                Docking,
            )>();
        }
        if failed || !owns_task {
            for (presentation, mut controller) in &mut outcomes.controllers {
                if (owns_task && presentation.gameplay_entity == actor)
                    || (owns_partner && presentation.gameplay_entity == owner.target)
                {
                    controller.playback_state = AnimationPlaybackState::Paused;
                }
            }
        }
        commands.entity(actor).remove::<SynchronizedBehaviorOwner>();
    }
    // An owner's destruction must not suppress its surviving partner forever.
    for (target, participant) in &participants {
        if !owners.contains(participant.owner) {
            commands.entity(target).remove::<(
                SynchronizedBehaviorParticipant,
                BehaviorTaskExecutionState,
                PendingBehaviorTaskFailure,
                PendingBehaviorAnimationClipCompletion,
                PendingBehaviorDockingCompletion,
                crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
                Destination,
                Docking,
            )>();
            for (presentation, mut controller) in &mut outcomes.controllers {
                if presentation.gameplay_entity == target {
                    controller.playback_state = AnimationPlaybackState::Paused;
                }
            }
        }
    }
}
