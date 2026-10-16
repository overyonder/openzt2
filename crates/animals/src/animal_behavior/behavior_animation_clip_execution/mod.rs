use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_data::behavior::{
    action::animation::BehaviorAnimationPlaybackSpeed, action_record::BehaviorAction,
};
use openzt2_world_state::animation_graph::{
    animation_graph_playback_state_types::AnimationGraphPlaybackState,
    animation_graph_transition_operations::{
        begin_requested_animation_graph_node_transition, find_animation_graph_transition_route,
    },
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

/// The tick an authored play time ends for one clip request.
#[derive(Component, Debug, Clone)]
pub(super) struct TimedBehaviorAnimationClip {
    request_id: usize,
    duration_ticks: u64,
    started: Option<(u64, u64)>,
    return_node: Option<String>,
    returning: bool,
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
            Option<&mut openzt2_zoo_state::guests::guest_simulation_types::GuestRng>,
        ),
        (
            Without<PendingBehaviorAnimationClipCompletion>,
            Without<PendingBehaviorTaskFailure>,
            Without<TimedBehaviorAnimationClip>,
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
                duration_ticks: until_tick.saturating_sub(simulation_clock.tick),
                started: None,
                return_node: None,
                returning: false,
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
    asset_server: Res<AssetServer>,
    mut animation_sets: ResMut<Assets<AnimationSetAsset>>,
    mut presentations: Query<(
        &AnimationPresentationOwner,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
    )>,
    mut cancellations: MessageWriter<openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest>,
    mut started_requests: MessageReader<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted>,
    mut behavior_tasks: Query<(
        Entity,
        &mut TimedBehaviorAnimationClip,
        Option<&mut BehaviorTaskExecutionState>,
        Option<&PendingBehaviorAnimationClipCompletion>,
        Has<PendingBehaviorTaskFailure>,
    )>,
) {
    for started in started_requests.read() {
        if let Ok((_, mut timed, _, _, _)) =
            behavior_tasks.get_mut(started.animation_subject_entity)
        {
            if timed.request_id == started.request_id && timed.started.is_none() {
                timed.return_node.clone_from(&started.previous_graph_node);
                timed.started = Some((
                    started.playback_generation,
                    simulation_clock.tick.saturating_add(timed.duration_ticks),
                ));
            }
        }
    }
    for (actor, mut timed, task, pending, failing) in &mut behavior_tasks {
        let owns_task = !failing
            && task.as_ref().zip(pending).is_some_and(|(task, pending)| {
                pending.request_id == timed.request_id && pending.still_owns_exact_task_frame(task)
            });
        if !owns_task {
            cancellations.write(openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest {
                animation_subject_entity: actor, request_id: timed.request_id,
            });
            for (owner, mut controller, mut graph) in &mut presentations {
                if owner.gameplay_entity == actor
                    && graph.route_owner.is_some_and(|owner| {
                        owner.request_id == timed.request_id
                            && owner.playback_generation == controller.playback_generation
                    })
                {
                    graph.route_owner = None;
                    graph.remaining_transition_nodes.clear();
                    graph.active_animation_graph_transition = None;
                    controller.playback_generation = controller.playback_generation.wrapping_add(1);
                    controller.playback_state = openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackState::Finished;
                }
            }
            commands
                .entity(actor)
                .remove::<TimedBehaviorAnimationClip>();
            continue;
        }
        let mut waiting = false;
        let mut succeeded = false;
        let mut matched = false;
        for (owner, mut controller, mut graph) in &mut presentations {
            if owner.gameplay_entity != actor {
                continue;
            }
            if timed.returning {
                let Some(route_owner) = graph.route_owner.filter(|owner| {
                    owner.request_id == timed.request_id
                        && owner.playback_generation == controller.playback_generation
                        && controller.explicit_clip_request_id.is_none()
                }) else {
                    continue;
                };
                matched = true;
                let target_matches = animation_sets
                    .get(&controller.animation_set_asset)
                    .and_then(|set| {
                        set.animation_graph_nodes
                            .get(route_owner.target_node as usize)
                    })
                    .is_some_and(|node| {
                        timed.return_node.as_ref() == Some(&node.animation_graph_node_asset_key)
                    });
                if !target_matches {
                    continue;
                }
                waiting |= graph.active_animation_graph_transition.is_some();
                succeeded |= graph.active_animation_graph_transition.is_none()
                    && graph.remaining_transition_nodes.is_empty()
                    && graph.current_animation_graph_node_index == route_owner.target_node;
                continue;
            }
            if controller.explicit_clip_request_id != Some(timed.request_id) {
                continue;
            }
            matched = true;
            let Some((generation, until_tick)) = timed.started else {
                waiting = true;
                continue;
            };
            if generation != controller.playback_generation {
                continue;
            }
            if simulation_clock.tick < until_tick {
                waiting = true;
                continue;
            }
            let Some(mut set) = animation_sets.get_mut(&controller.animation_set_asset) else {
                continue;
            };
            let target = timed
                .return_node
                .as_ref()
                .and_then(|name| {
                    set.animation_graph_nodes
                        .iter()
                        .position(|node| &node.animation_graph_node_asset_key == name)
                })
                .and_then(|index| u32::try_from(index).ok());
            if let Some(target) = target {
                if let Some(route) = find_animation_graph_transition_route(
                    &set,
                    graph.current_animation_graph_node_index,
                    target,
                ) {
                    let mut source = graph.current_animation_graph_node_index;
                    let mut clip_choices = Vec::new();
                    for next in &route {
                        let source_name = &set.animation_graph_nodes[source as usize]
                            .animation_graph_node_asset_key;
                        let target_node = &set.animation_graph_nodes[*next as usize];
                        if let Some(edge) = set.animation_graph_edges.iter().find(|edge| {
                            &edge.source_animation_graph_node_asset_key == source_name
                                && edge.target_animation_graph_node_asset_key
                                    == target_node.animation_graph_node_asset_key
                        }) {
                            if !edge.transition_animation_clip_asset_keys.is_empty() {
                                clip_choices
                                    .push(edge.transition_animation_clip_asset_keys.clone());
                            }
                        }
                        clip_choices.push(target_node.animation_clip_asset_keys.clone());
                        source = *next;
                    }
                    let mut loading = false;
                    let mut failed = false;
                    for choices in clip_choices {
                        // Graph execution selects one available alternative per edge/node.
                        if choices
                            .iter()
                            .any(|clip| set.find_animation_clip_record_by_asset_key(clip).is_some())
                        {
                            continue;
                        }
                        // Try the authored alternatives in order, requesting only one
                        // usable source at a time instead of loading every alternative.
                        if choices
                            .iter()
                            .any(|clip| set.request_animation_clip_asset_load(&asset_server, clip))
                        {
                            loading = true;
                        } else {
                            failed = true;
                        }
                    }
                    if failed {
                        break;
                    }
                    if loading {
                        waiting = true;
                        continue;
                    }
                    if begin_requested_animation_graph_node_transition(
                        &set,
                        target,
                        &mut controller,
                        &mut graph,
                        None,
                        false,
                    )
                    .is_ok()
                    {
                        timed.returning = true;
                        graph.route_owner = Some(openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphRouteOwner {
                            request_id: timed.request_id,
                            target_node: target,
                            playback_generation: controller.playback_generation,
                        });
                        waiting |= graph.active_animation_graph_transition.is_some();
                        succeeded |= graph.active_animation_graph_transition.is_none()
                            && graph.remaining_transition_nodes.is_empty()
                            && graph.current_animation_graph_node_index == target;
                        continue;
                    }
                }
            }
        }
        // Demand loading has not accepted this request yet; its timer has not begun.
        if !matched && timed.started.is_none() && !timed.returning {
            continue;
        }
        if waiting {
            continue;
        }
        if !succeeded {
            cancellations.write(openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest {
                animation_subject_entity: actor, request_id: timed.request_id,
            });
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            commands.entity(actor).remove::<(
                TimedBehaviorAnimationClip,
                PendingBehaviorAnimationClipCompletion,
            )>();
            warn!(
                ?actor,
                request_id = timed.request_id,
                return_node = ?timed.return_node,
                started = ?timed.started,
                returning = timed.returning,
                "timed behavior animation could not complete its authored return route"
            );
            continue;
        }
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
        if failing {
            commands
                .entity(actor)
                .remove::<TimedBehaviorAnimationClip>();
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
