use super::super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use super::SynchronizedBehaviorParticipant;
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::behavior_synchronized_set_types::{
    SynchronizedBehaviorOperation, SynchronizedBehaviorPartner, SynchronizedBehaviorSubject,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    abandon_behavior_task_for_reconsideration,
    continuations::BehaviorContinuationFrames,
    running_actions::{
        ResumeSuspendedBehaviorInstruction, RunBehaviorAnimationClip, RunningBehaviorInstruction,
    },
    BehaviorTaskExecutionState, BehaviorTaskFailed, BehaviorTaskFinished,
    PendingBehaviorTaskFailure,
};

pub(super) fn finish_or_cancel_synchronized_behavior_sets(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    mut operations: Query<(
        Entity,
        &mut SynchronizedBehaviorOperation,
        Option<&SynchronizedBehaviorSubject>,
        Option<&SynchronizedBehaviorPartner>,
    )>,
    participants: Query<&SynchronizedBehaviorParticipant>,
    frames: BehaviorContinuationFrames,
    tasks: Query<(&BehaviorTaskExecutionState, Has<PendingBehaviorTaskFailure>)>,
    looping_clips: Query<(&RunBehaviorAnimationClip, &RunningBehaviorInstruction)>,
    presentations: Query<&AnimationPresentations>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for (actor, execution_id, success) in finished
        .read()
        .map(|event| (event.actor, event.execution_id, true))
        .chain(
            failed
                .read()
                .map(|event| (event.actor, event.execution_id, false)),
        )
    {
        let Some(operation) = participants
            .get(actor)
            .ok()
            .and_then(|target| target.iter().next())
        else {
            continue;
        };
        if let Ok((_, mut operation, _, _)) = operations.get_mut(operation) {
            if operation.target_execution_id == execution_id {
                operation.target_outcome = Some(success);
            }
        }
    }
    for (operation, state, subject, partner) in &operations {
        let actor = subject.map(|relation| relation.0);
        let target = partner.map(|relation| relation.0);
        let actor_task = actor.and_then(|actor| tasks.get(actor).ok());
        let owns_subject = actor_task.as_ref().is_some_and(|(task, _)| {
            task.execution_id == state.execution_id
                && actor.is_some_and(|actor| frames.owns(actor, task, &state.origin, state.depth))
                && (target.is_none() || task.target() == target)
        });
        let partner_task = target.and_then(|target| tasks.get(target).ok());
        let owns_partner = target.is_some()
            && partner_task
                .as_ref()
                .is_none_or(|(task, _)| task.execution_id == state.target_execution_id);
        let subject_finished = owns_subject
            && actor_task
                .as_ref()
                .is_some_and(|(task, _)| task.depth == state.depth);
        let failed = actor_task.as_ref().is_some_and(|(_, failed)| *failed)
            || !owns_partner
            || state.target_outcome == Some(false);
        // An untimed looping wait ends when the subject's set returns.
        let partner_waits = target
            .and_then(|target| looping_clips.get(target).ok())
            .zip(partner_task.as_ref())
            .is_some_and(|((clip, instruction), (task, _))| {
                owns_partner
                    && instruction.is_current(task)
                    && clip.0.looping
                    && clip.0.play_duration_seconds.is_none()
            });
        if owns_subject
            && !failed
            && !(subject_finished && (state.target_outcome == Some(true) || partner_waits))
        {
            continue;
        }
        let partner_has_task = partner_task.is_some();
        if let Some(actor) = actor.filter(|_| owns_subject) {
            commands.trigger(ResumeSuspendedBehaviorInstruction {
                actor,
                execution_id: state.execution_id,
                origin: state.origin.clone(),
                depth: state.depth,
                instruction: state.origin.clone(),
                tick: clock.tick,
                advance: !failed,
            });
            if failed {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
        }
        if let Some(target) = target.filter(|_| owns_partner) {
            if partner_has_task {
                abandon_behavior_task_for_reconsideration(target, &mut commands);
            }
        }
        if failed || !owns_subject {
            for subject in [
                actor.filter(|_| owns_subject),
                target.filter(|_| owns_partner),
            ]
            .into_iter()
            .flatten()
            {
                for controller in
                    AnimationPresentations::for_subject(subject, presentations.get(subject).ok())
                {
                    if let Ok(mut player) = players.get_mut(controller) {
                        player.pause_all();
                    }
                }
            }
        }
        commands.entity(operation).despawn();
    }
}
