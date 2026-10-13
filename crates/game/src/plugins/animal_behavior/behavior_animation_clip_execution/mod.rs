use bevy::prelude::*;
use openzt2_game_data::behavior::{
    action::animation::BehaviorAnimationPlaybackSpeed, action_record::BehaviorAction,
};

use crate::plugins::{
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
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

/// The tick an authored play time ends for one clip request.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct TimedBehaviorAnimationClip {
    request_id: usize,
    until_tick: u64,
}

pub(super) fn start_current_behavior_animation_clip_actions(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&mut super::behavior_random_stream_state::BehaviorRandomStream>,
            Option<&mut crate::plugins::guests::guest_simulation_types::GuestRng>,
        ),
        (
            Without<PendingBehaviorAnimationClipCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    mut animation_clip_playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    definitions: Option<Res<WorldDefinitions>>,
    definition_assets: Option<Res<Assets<WorldDefinitionAsset>>>,
) {
    for (actor, mut behavior_task, mut behavior_random, mut guest_random) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::AnimationClip(animation)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        // A play time repeats the clip until it elapses.
        let play_until_tick = match animation.play_duration_seconds {
            None => None,
            Some(seconds) => {
                let Some(fixed_hz) = definitions
                    .as_ref()
                    .zip(definition_assets.as_ref())
                    .and_then(|(definitions, assets)| definitions.get(assets))
                    .map(|definitions| definitions.timing().fixed_hz)
                else {
                    super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                        actor, &mut commands,
                    );
                    continue;
                };
                let ticks = (f64::from(seconds) * f64::from(fixed_hz)).ceil();
                // Lowering accepts only finite, non-negative play times.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let ticks = ticks as u64;
                Some(simulation_clock.tick.saturating_add(ticks))
            }
        };
        // A speed range is sampled each time the clip starts, from the
        // actor's own behavior random stream.
        let playback_speed = match animation.playback_speed {
            None => 1.0,
            Some(BehaviorAnimationPlaybackSpeed::Fixed(speed)) => speed,
            Some(BehaviorAnimationPlaybackSpeed::Range { minimum, maximum }) => {
                let unit = behavior_random
                    .as_deref_mut()
                    .map(|random| random.0.unit_f32())
                    .or_else(|| {
                        guest_random
                            .as_deref_mut()
                            .map(|random| random.0.unit_f32())
                    });
                let Some(unit) = unit else {
                    super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                        actor, &mut commands,
                    );
                    continue;
                };
                (maximum - minimum).mul_add(unit, minimum)
            }
        };
        // Lowering accepts only finite positive speeds and blend times.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let playback_speed_permille = (playback_speed * 1000.0)
            .round()
            .clamp(1.0, f32::from(i16::MAX)) as i16;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let blend_duration_milliseconds = animation.blend_duration_seconds.map_or(0, |seconds| {
            (seconds * 1000.0).round().min(u32::MAX as f32) as u32
        });
        // interruptFlag: an explicit request already replaces the current
        // animation at once. interruptible: tasks are not yet cut short by a
        // newly chosen task, so the clip runs its course. groundFit: actors
        // are not yet fitted to the terrain slope, so disabling it changes
        // nothing.
        let request_id = animation_clip_playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: actor,
            animation_clip_asset_key: animation.animation_clip_asset_key.clone(),
            blend_duration_milliseconds,
            playback_speed_permille,
            playback_repetition_policy: if animation.looping || play_until_tick.is_some() {
                AnimationPlaybackRepetitionPolicy::Loop
            } else {
                AnimationPlaybackRepetitionPolicy::PlayOnce
            },
        });
        if let Some(until_tick) = play_until_tick {
            commands.entity(actor).insert(TimedBehaviorAnimationClip {
                request_id: request_id.id,
                until_tick,
            });
        }
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
        debug!(?actor, program = ?behavior_task.program, request_id = request_id.id,
            clip = %animation.animation_clip_asset_key, "behavior clip requested");
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
            debug!(?actor, clip = %rejected.animation_clip_asset_key,
                request_id = rejected.request_id,
                "behavior clip rejected: no ready animation presentation for this actor");
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
        debug!(
            ?actor,
            request_id = pending.request_id,
            "behavior clip completed"
        );
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

/// Ends a clip action once its authored play time has elapsed.
pub(super) fn finish_behavior_animation_clip_actions_after_play_time(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (
            Entity,
            &TimedBehaviorAnimationClip,
            Option<&mut BehaviorTaskExecutionState>,
            Option<&PendingBehaviorAnimationClipCompletion>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for (actor, timed, task, pending) in &mut behavior_tasks {
        let (Some(mut task), Some(pending)) = (task, pending) else {
            commands
                .entity(actor)
                .remove::<TimedBehaviorAnimationClip>();
            continue;
        };
        if pending.request_id != timed.request_id || !pending.still_owns_exact_task_frame(&task) {
            commands
                .entity(actor)
                .remove::<TimedBehaviorAnimationClip>();
            continue;
        }
        if simulation_clock.tick < timed.until_tick {
            continue;
        }
        let current_clip = behavior_document_assets
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
            .and_then(|action| pending.clip_key(action))
            .is_some();
        if current_clip {
            debug!(
                ?actor,
                request_id = pending.request_id,
                "behavior clip play time elapsed"
            );
            advance_behavior_task_to_next_action(&mut task, simulation_clock.tick);
        }
        commands.entity(actor).remove::<(
            TimedBehaviorAnimationClip,
            PendingBehaviorAnimationClipCompletion,
        )>();
    }
}

#[cfg(test)]
mod tests;
