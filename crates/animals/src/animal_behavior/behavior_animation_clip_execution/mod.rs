use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_data::behavior::action::animation::{
    BehaviorAnimationClipAction, BehaviorAnimationPlaybackSpeed,
};
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::{
    AnimationClipPlaybackRequestStarted, AnimationReturnRouteOutcome, AnimationReturnRouteRequest,
    CancelAnimationClipPlaybackRequest,
};
use openzt2_world_state::animation_graph::{
    animation_graph_playback_state_types::AnimationGraphPlaybackState,
    animation_presentation_relationship_types::AnimationPresentations,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorAnimationClip, RunBehaviorAnimationSwitch, RunningBehaviorInstruction,
};

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipActionSettings;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

/// The tick an authored play time ends for one clip request.
#[derive(Component, Debug, Clone)]
pub(super) struct TimedBehaviorAnimationClip {
    request_id: usize,
    duration_ticks: u64,
    started: Option<(Entity, u64, u64)>,
    return_node: Option<String>,
    return_observer: Option<Entity>,
}

pub(super) fn start_current_behavior_animation_clip_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    simulation_clock: Single<&ZooClock>,
    mut behavior_tasks: Query<
        (
            Entity,
            (
                &mut BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                Option<&RunBehaviorAnimationClip>,
                Option<&RunBehaviorAnimationSwitch>,
            ),
            Option<&mut super::behavior_random_stream_state::BehaviorRandomStream>,
            Option<&mut openzt2_zoo_state::guests::guest_simulation_types::GuestRng>,
        ),
        (
            Without<PendingBehaviorAnimationClipCompletion>,
            Without<PendingBehaviorTaskFailure>,
            Without<TimedBehaviorAnimationClip>,
            Or<(
                With<RunBehaviorAnimationClip>,
                With<RunBehaviorAnimationSwitch>,
            )>,
        ),
    >,
    mut animation_clip_playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    definitions: Option<Res<WorldDefinitions>>,
    definition_assets: Option<Res<Assets<WorldDefinitionAsset>>>,
    animation_sources: (
        Option<Res<Assets<AnimationSetAsset>>>,
        Query<&AnimationPresentations>,
        Query<(&AnimationPlaybackController, &AnimationGraphPlaybackState)>,
    ),
) {
    for (
        actor,
        (mut behavior_task, instruction, running_action, switch),
        mut behavior_random,
        mut guest_random,
    ) in behavior_action_actors(&mut behavior_tasks, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(simulation_clock.tick)
        {
            continue;
        }
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        if !instruction.is_current(&behavior_task) {
            continue;
        }
        let switched_clip;
        let animation = if let Some(clip) = running_action {
            &clip.0
        } else {
            let Some(switch) = switch else {
                continue;
            };
            let (sets, presentations, controllers) = &animation_sources;
            let key = AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
                .find_map(|presentation| controllers.get(presentation).ok())
                .and_then(|(controller, state)| {
                    let set = sets.as_ref()?.get(&controller.animation_set_asset)?;
                    let node = set
                        .animation_graph_nodes
                        .get(state.current_animation_graph_node_index as usize)?;
                    switch
                        .0
                        .clips_by_node
                        .iter()
                        .find(|(name, _)| {
                            name.eq_ignore_ascii_case(&node.animation_graph_node_asset_key)
                        })
                        .map(|(_, clip)| clip)
                })
                .or(switch.0.default_clip.as_ref());
            let Some(key) = key else {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                    actor, &mut commands,
                );
                continue;
            };
            switched_clip = BehaviorAnimationClipAction {
                animation_clip_asset_key: key.clone(),
                looping: false,
                physical_object: None,
                play_duration_seconds: None,
                interrupts_current_animation: false,
                interruptible: false,
                ground_fit: None,
                blend_duration_seconds: None,
                playback_speed: None,
            };
            &switched_clip
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
        let binder = animation.physical_object.as_deref().map(|name| {
            let name = openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(name);
            openzt2_game_data::AssetId::from_key(&name)
        });
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
            action_settings: Some(AnimationClipActionSettings {
                binder,
                ground_fit: animation.ground_fit,
                interrupts_current_animation: animation.interrupts_current_animation,
                interruptible: animation.interruptible,
            }),
        });
        if let Some(until_tick) = play_until_tick {
            commands.entity(actor).insert(TimedBehaviorAnimationClip {
                request_id: request_id.id,
                duration_ticks: until_tick.saturating_sub(simulation_clock.tick),
                started: None,
                return_node: None,
                return_observer: None,
            });
        }
        commands
            .entity(actor)
            .insert(PendingBehaviorAnimationClipCompletion {
                instruction: instruction.id(),
                request_id: request_id.id,
                clip_key: animation.animation_clip_asset_key.clone(),
                target: behavior_task.target,
                repeats: false,
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
    waiting_actors: Query<(
        &BehaviorTaskExecutionState,
        &PendingBehaviorAnimationClipCompletion,
        &RunningBehaviorInstruction,
    )>,
) {
    for rejected in rejected_animations.read() {
        let actor = rejected.animation_subject_entity;
        let matches_current_action =
            waiting_actors
                .get(actor)
                .is_ok_and(|(task, pending, instruction)| {
                    pending.request_id == rejected.request_id
                        && pending.owns(task, instruction)
                        && pending.clip_key == rejected.animation_clip_asset_key
                });
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
    animation_controllers: Query<&AnimationPresentationOwner>,
    simulation_clock: Single<&ZooClock>,
    mut behavior_tasks: Query<
        (
            &mut BehaviorTaskExecutionState,
            &PendingBehaviorAnimationClipCompletion,
            &RunningBehaviorInstruction,
            Option<&openzt2_zoo_state::behavior_task_execution_types::running_actions::TimedBehaviorAnimationEffect>,
            Option<&super::behavior_animation_key_execution::CrossedBehaviorAnimationEffect>,
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
        let Ok((mut behavior_task, pending, instruction, timed_effect, key)) =
            behavior_tasks.get_mut(actor)
        else {
            continue;
        };
        if completed_animation.explicit_clip_request_id != Some(pending.request_id)
            || !pending.owns(&behavior_task, instruction)
        {
            continue;
        }
        if let Some(effect) = timed_effect {
            if key.is_none_or(|key| !key.belongs_to(instruction)) {
                if matches!(effect.0, openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorAnimationEffect::Detach { .. }) {
                    commands.entity(actor).insert(
                        super::behavior_animation_key_execution::CrossedBehaviorAnimationEffect::detach(instruction),
                    );
                } else {
                    super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    commands.entity(actor).try_remove::<PendingBehaviorAnimationClipCompletion>();
                }
            }
            continue;
        }
        debug!(
            ?actor,
            request_id = pending.request_id,
            "behavior clip completed"
        );
        commands
            .entity(actor)
            .remove::<PendingBehaviorAnimationClipCompletion>();
        if pending.repeats {
            behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
        } else {
            instruction.complete(actor, simulation_clock.tick, &mut commands);
        }
    }
}

/// The behavior owns its hold time and waits for the animation owner's return result.
pub(super) fn finish_behavior_animation_clip_actions_after_play_time(
    mut commands: Commands,
    simulation_clock: Single<&ZooClock>,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
    mut started: MessageReader<AnimationClipPlaybackRequestStarted>,
    mut returns: MessageWriter<AnimationReturnRouteRequest>,
    mut actors: Query<(
        Entity,
        &mut TimedBehaviorAnimationClip,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Option<&PendingBehaviorAnimationClipCompletion>,
        Has<PendingBehaviorTaskFailure>,
    )>,
) {
    for playback in started.read() {
        let Ok((_, mut timed, ..)) = actors.get_mut(playback.animation_subject_entity) else {
            continue;
        };
        if timed.request_id == playback.request_id && timed.started.is_none() {
            timed.return_node.clone_from(&playback.previous_graph_node);
            timed.started = Some((
                playback.controller_entity,
                playback.playback_generation,
                simulation_clock.tick.saturating_add(timed.duration_ticks),
            ));
        }
    }
    for (actor, mut timed, task, pending, failing) in &mut actors {
        let owns_task = task
            .zip(pending)
            .is_some_and(|((task, instruction), pending)| {
                !failing
                    && pending.request_id == timed.request_id
                    && pending.owns(task, instruction)
            });
        if !owns_task {
            if let Some(observer) = timed.return_observer {
                commands.entity(observer).try_despawn();
            }
            cancellations.write(CancelAnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                request_id: timed.request_id,
            });
            commands
                .entity(actor)
                .try_remove::<TimedBehaviorAnimationClip>();
            continue;
        }
        if timed.return_observer.is_some() {
            continue;
        }
        let Some((controller, generation, until)) = timed.started else {
            continue;
        };
        if simulation_clock.tick < until {
            continue;
        }
        let Some(node) = timed.return_node.clone() else {
            cancellations.write(CancelAnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                request_id: timed.request_id,
            });
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            commands.entity(actor).try_remove::<(
                TimedBehaviorAnimationClip,
                PendingBehaviorAnimationClipCompletion,
            )>();
            continue;
        };
        let observer = commands
            .spawn(Observer::new(consume_animation_return_route).with_entity(actor))
            .id();
        timed.return_observer = Some(observer);
        returns.write(AnimationReturnRouteRequest {
            subject: actor,
            controller,
            request_id: timed.request_id,
            generation,
            node,
        });
    }
}

fn consume_animation_return_route(
    outcome: On<AnimationReturnRouteOutcome>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
    actors: Query<(
        &TimedBehaviorAnimationClip,
        &BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        &PendingBehaviorAnimationClipCompletion,
        Has<PendingBehaviorTaskFailure>,
    )>,
) {
    let actor = outcome.subject;
    let Ok((timed, task, instruction, pending, failing)) = actors.get(actor) else {
        return;
    };
    if timed.return_observer != Some(outcome.observer()) || timed.request_id != outcome.request_id {
        return;
    }
    commands.entity(outcome.observer()).try_despawn();
    commands.entity(actor).try_remove::<(
        TimedBehaviorAnimationClip,
        PendingBehaviorAnimationClipCompletion,
    )>();
    if !failing && pending.request_id == timed.request_id && pending.owns(task, instruction) {
        if outcome.succeeded {
            instruction.complete(actor, clock.tick, &mut commands);
        } else {
            cancellations.write(CancelAnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                request_id: timed.request_id,
            });
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}

#[cfg(test)]
mod tests;
