use bevy::prelude::*;
use openzt2_game_data::behavior::action::feedback::BehaviorFeedbackEntry;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackDispatched;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackLine;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackLog;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

pub(super) fn dispatch_current_authored_behavior_feedback_and_advance(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<(Entity, &mut BehaviorTaskExecutionState)>,
    mut dispatched_feedback: MessageWriter<BehaviorFeedbackDispatched>,
) {
    for (actor, mut behavior_task) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::Feedback(feedback)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        let dispatch = BehaviorFeedbackDispatched {
            actor,
            document: behavior_task.document.clone(),
            declaration: behavior_task.declaration,
            phase: behavior_task.phase,
            action: behavior_task.action,
        };
        debug!(
            actor = ?dispatch.actor,
            document = ?dispatch.document.id(),
            declaration = dispatch.declaration,
            phase = ?dispatch.phase,
            action = dispatch.action,
            entries = feedback.entries.len(),
            "dispatched authored behavior feedback"
        );
        dispatched_feedback.write(dispatch);
        advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
    }
}

/// Keeps each actor's latest authored action and its recent thoughts for its
/// information panel, and clears the action once no task runs.
pub(super) fn record_dispatched_behavior_feedback(
    mut commands: Commands,
    mut dispatched: MessageReader<BehaviorFeedbackDispatched>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    fixed_time: Res<Time<Fixed>>,
    tasks: Query<&BehaviorTaskExecutionState>,
    mut logs: Query<&mut BehaviorFeedbackLog>,
    mut finished: RemovedComponents<BehaviorTaskExecutionState>,
) {
    for actor in finished.read() {
        if let Ok(mut log) = logs.get_mut(actor) {
            if log.action.is_some() && !tasks.contains(actor) {
                log.action = None;
            }
        }
    }
    let ticks_per_second = 1.0 / fixed_time.timestep().as_secs_f64().max(1e-6);
    for dispatch in dispatched.read() {
        let Some(BehaviorAction::Feedback(feedback)) = behavior_document_assets
            .get(&dispatch.document)
            .and_then(|document| feedback_action(document, dispatch))
        else {
            continue;
        };
        let task = tasks.get(dispatch.actor).ok();
        let named = |use_entity: bool, use_target: bool, use_object: bool| {
            if use_target {
                task.and_then(BehaviorTaskExecutionState::target)
            } else if use_object {
                task.and_then(|task| task.object)
            } else if use_entity {
                Some(dispatch.actor)
            } else {
                None
            }
        };
        let mut log = logs.get(dispatch.actor).cloned().unwrap_or_default();
        for entry in &feedback.entries {
            match entry {
                BehaviorFeedbackEntry::Action {
                    localization_key,
                    use_entity_name,
                    use_target_name,
                    use_object_name,
                    ..
                } => {
                    log.action = Some(BehaviorFeedbackLine {
                        localization_key: localization_key.clone(),
                        named: named(*use_entity_name, *use_target_name, *use_object_name),
                    });
                }
                BehaviorFeedbackEntry::Thought {
                    localization_key,
                    timeout_seconds,
                    use_entity_name,
                    use_target_name,
                    use_target_object,
                    ..
                } => {
                    let timeout_ticks =
                        (f64::from(timeout_seconds.unwrap_or(60)) * ticks_per_second).ceil() as u64;
                    log.think(
                        BehaviorFeedbackLine {
                            localization_key: localization_key.clone(),
                            named: named(*use_entity_name, *use_target_name, *use_target_object),
                        },
                        simulation_clock.tick,
                        simulation_clock.tick.saturating_add(timeout_ticks),
                    );
                }
                BehaviorFeedbackEntry::Message { .. } | BehaviorFeedbackEntry::Emoticon { .. } => {}
            }
        }
        if let Ok(mut current) = logs.get_mut(dispatch.actor) {
            if *current != log {
                *current = log;
            }
        } else if let Ok(mut actor) = commands.get_entity(dispatch.actor) {
            actor.insert(log);
        }
    }
}

fn feedback_action<'a>(
    document: &'a BehaviorDocumentAsset,
    dispatch: &BehaviorFeedbackDispatched,
) -> Option<&'a BehaviorAction> {
    use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
    let phase = match dispatch.phase {
        BehaviorTaskExecutionPhase::Set => {
            &document
                .behavior_set_at_index(dispatch.declaration)?
                .actions
        }
        BehaviorTaskExecutionPhase::Execution => {
            &document
                .behavior_task_at_index(dispatch.declaration)?
                .execution
        }
        BehaviorTaskExecutionPhase::Completion => {
            &document
                .behavior_task_at_index(dispatch.declaration)?
                .completion
        }
        BehaviorTaskExecutionPhase::Failure => {
            &document
                .behavior_task_at_index(dispatch.declaration)?
                .failure
        }
    };
    phase.lowered_actions().get(dispatch.action)
}
