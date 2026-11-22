//! Uses the normal playback request and completion owner.
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;

use super::{
    behavior_random_choice_execution::select_next_authored_random_choice,
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};
use bevy::prelude::*;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorRandomAnimation, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;

pub(super) fn start_next_random_behavior_animation(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut actors: Query<
        (
            Entity,
            (
                &mut BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorRandomAnimation,
            ),
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
    for (actor, (mut task, instruction, running_action), mut guest_random, mut behavior_random) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(&task) {
            continue;
        }
        let RunBehaviorRandomAnimation {
            weighted_clips,
            minimum_plays,
            maximum_plays,
            looping,
        } = running_action;
        let Some(random) = guest_random
            .as_deref_mut()
            .map(|rng| &mut rng.0)
            .or_else(|| behavior_random.as_deref_mut().map(|rng| &mut rng.0))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let mut repetitions = task.repetitions;
        match select_next_authored_random_choice(
            weighted_clips,
            *minimum_plays,
            *maximum_plays,
            *looping,
            &mut repetitions,
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
                        instruction: instruction.id(),
                        request_id: request_id.id,
                        clip_key: weighted_clips[index].0.clone(),
                        target: task.target,
                        repeats: true,
                    });
                task.next_action_tick = clock.tick.saturating_add(1);
                task.repetitions = repetitions;
            }
            Ok(None) => instruction.complete(actor, clock.tick, &mut commands),
            Err(()) => mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands),
        }
    }
}
