use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        animation_graph::{
            animation_graph_playback_message_types::{
                AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
            },
            animation_presentation_relationship_types::AnimationPresentationOwner,
        },
        animation_playback::animation_event_message_types::AnimationCompleted,
        animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
            PendingBehaviorTaskFailure,
        },
        simulation_time::simulation_clock_types::ZooClock,
    },
};

pub(super) fn start_current_behavior_animation_clip_actions(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (Entity, &mut BehaviorTaskExecutionState),
        (
            Without<PendingBehaviorAnimationClipCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    mut animation_clip_playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    for (actor, mut behavior_task) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::AnimationClip(animation)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        let request_id = animation_clip_playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: actor,
            animation_clip_asset_key: animation.animation_clip_asset_key.clone(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: if animation.looping {
                AnimationPlaybackRepetitionPolicy::Loop
            } else {
                AnimationPlaybackRepetitionPolicy::PlayOnce
            },
        });
        commands
            .entity(actor)
            .insert(PendingBehaviorAnimationClipCompletion {
                execution_id: behavior_task.execution_id,
                request_id: request_id.id,
                origin: behavior_task.create_return_frame_after_action(behavior_task.action),
                stack_depth: behavior_task.stack.len(),
                target: behavior_task.target,
                random_choice: None,
            });
        behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
    }
}

/// A refused clip cannot produce a completion event. Propagate that refusal to
/// the task's authored failure branch instead of retaining an eternal wait.
pub(super) fn fail_behavior_animation_actions_after_playback_rejection(
    mut commands: Commands,
    mut rejected_animations: MessageReader<AnimationClipPlaybackRequestRejected>,
    behavior_documents: Res<Assets<BehaviorDocumentAsset>>,
    waiting_actors: Query<(
        &BehaviorTaskExecutionState,
        &PendingBehaviorAnimationClipCompletion,
    )>,
) {
    for rejected in rejected_animations.read() {
        let actor = rejected.animation_subject_entity;
        let matches_current_action = waiting_actors
            .get(actor)
            .ok()
            .and_then(|(task, pending)| {
                if pending.request_id != rejected.request_id
                    || !pending.still_owns_exact_task_frame(task)
                {
                    return None;
                }
                behavior_documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, task))
                    .and_then(|action| pending.clip_key(action))
            })
            .is_some_and(|clip| clip == rejected.animation_clip_asset_key);
        if matches_current_action {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                actor, &mut commands,
            );
            commands
                .entity(actor)
                .remove::<PendingBehaviorAnimationClipCompletion>();
        }
    }
}

pub(super) fn finish_behavior_animation_clip_actions_after_playback_completion(
    mut commands: Commands,
    mut completed_animations: MessageReader<AnimationCompleted>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    animation_controllers: Query<&AnimationPresentationOwner>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (
            &mut BehaviorTaskExecutionState,
            &PendingBehaviorAnimationClipCompletion,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for completed_animation in completed_animations.read() {
        let Ok(animation_owner) =
            animation_controllers.get(completed_animation.animation_playback_controller_entity)
        else {
            continue;
        };
        let actor = animation_owner.gameplay_entity;
        let Ok((mut behavior_task, pending)) = behavior_tasks.get_mut(actor) else {
            continue;
        };
        if completed_animation.explicit_clip_request_id != Some(pending.request_id)
            || !pending.still_owns_exact_task_frame(&behavior_task)
        {
            continue;
        }
        let action_matches_completed_clip = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
            .and_then(|action| pending.clip_key(action))
            .is_some();
        if !action_matches_completed_clip {
            continue;
        }
        if pending.random_choice.is_some() {
            behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
        } else {
            advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
        }
        commands
            .entity(actor)
            .remove::<PendingBehaviorAnimationClipCompletion>();
    }
}

#[cfg(test)]
mod tests;
