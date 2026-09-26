//! Random authored clips use the normal playback request and completion owner.

use super::{
    behavior_random_choice_execution::select_next_authored_random_choice,
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};
use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest,
        animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
            PendingBehaviorTaskFailure,
        },
        guests::guest_simulation_types::GuestRng,
        simulation_time::simulation_clock_types::ZooClock,
    },
};
use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

pub(super) fn start_next_random_behavior_animation(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&mut GuestRng>,
            Option<&mut BehaviorRandomStream>,
        ),
        (
            Without<PendingBehaviorAnimationClipCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    for (actor, mut task, mut guest_random, mut behavior_random) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::RandomAnimation {
            weighted_clips,
            minimum_plays,
            maximum_plays,
            looping,
        }) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(random) = guest_random
            .as_deref_mut()
            .map(|rng| &mut rng.0)
            .or_else(|| behavior_random.as_deref_mut().map(|rng| &mut rng.0))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        match select_next_authored_random_choice(
            weighted_clips,
            *minimum_plays,
            *maximum_plays,
            *looping,
            &mut task.repetitions,
            random,
        ) {
            Ok(Some(index)) => {
                let request_id = requests.write(AnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    animation_clip_asset_key: weighted_clips[index].0.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                });
                commands
                    .entity(actor)
                    .insert(PendingBehaviorAnimationClipCompletion {
                        execution_id: task.execution_id,
                        request_id: request_id.id,
                        origin: task.create_return_frame_after_action(task.action),
                        stack_depth: task.stack.len(),
                        target: task.target,
                        random_choice: Some(index),
                    });
                task.next_action_tick = clock.tick.saturating_add(1);
            }
            Ok(None) => advance_behavior_task_to_next_action(&mut task, clock.tick),
            Err(()) => mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands),
        }
    }
}
