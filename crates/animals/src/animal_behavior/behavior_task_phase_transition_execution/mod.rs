//! Return-stack, failure, completion and final-outcome transitions for behavior tasks.

use bevy::prelude::*;

#[cfg(test)]
mod tests;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::behavior::document::BehaviorTaskActionPhase;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

pub(super) fn advance_behavior_tasks_through_return_failure_completion_and_final_outcome(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    mut behavior_tasks: Query<(
        Entity,
        &mut BehaviorTaskExecutionState,
        Option<&PendingBehaviorTaskFailure>,
    )>,
    mut commands: Commands,
    simulation_clock: Res<ZooClock>,
    mut failed_behavior_tasks: MessageWriter<BehaviorTaskFailed>,
    mut finished_behavior_tasks: MessageWriter<BehaviorTaskFinished>,
) {
    for (actor_entity, mut behavior_task, pending_failure) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        if pending_failure.is_some() {
            if behavior_task.phase == BehaviorTaskExecutionPhase::Set {
                if let Some(return_frame) = behavior_task.stack.pop() {
                    behavior_task.program = return_frame.program;
                    behavior_task.document = return_frame.document;
                    behavior_task.declaration = return_frame.declaration;
                    behavior_task.phase = return_frame.phase;
                    behavior_task.action = return_frame.action;
                    behavior_task.repetitions = return_frame.repetitions;
                    behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
                    continue;
                }
            }
            let failure_actions = behavior_document_assets
                .get(&behavior_task.document)
                .and_then(|asset| asset.behavior_task_at_index(behavior_task.declaration))
                .map(|declaration| &declaration.failure);
            if behavior_task.phase != BehaviorTaskExecutionPhase::Failure
                && failure_actions.is_some_and(|actions| !actions.is_empty())
            {
                behavior_task.phase = BehaviorTaskExecutionPhase::Failure;
                behavior_task.action = 0;
                behavior_task.repetitions = 0;
                behavior_task.stack.clear();
                behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
                commands
                    .entity(actor_entity)
                    .remove::<PendingBehaviorTaskFailure>();
            } else {
                failed_behavior_tasks.write(BehaviorTaskFailed {
                    actor: actor_entity,
                    execution_id: behavior_task.execution_id,
                });
                commands.entity(actor_entity).remove::<(
                    BehaviorTaskExecutionState,
                    openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore,
                    PendingBehaviorTaskFailure,
                    PendingBehaviorAnimationClipCompletion,
                    PendingBehaviorDockingCompletion,
                    openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion,
                )>();
            }
            continue;
        }
        let Some(behavior_document_asset) = behavior_document_assets.get(&behavior_task.document)
        else {
            continue;
        };
        if find_current_behavior_task_action(behavior_document_asset, &behavior_task).is_some() {
            continue;
        }
        let current_phase = if behavior_task.phase == BehaviorTaskExecutionPhase::Set {
            behavior_document_asset
                .behavior_set_at_index(behavior_task.declaration)
                .map(|set| (set.name.as_str(), &set.actions))
        } else {
            behavior_document_asset
                .behavior_task_at_index(behavior_task.declaration)
                .and_then(|task| {
                    Some((
                        task.name.as_str(),
                        match behavior_task.phase {
                            BehaviorTaskExecutionPhase::Execution => &task.execution,
                            BehaviorTaskExecutionPhase::Completion => &task.completion,
                            BehaviorTaskExecutionPhase::Failure => &task.failure,
                            BehaviorTaskExecutionPhase::Set => return None,
                        },
                    ))
                })
        };
        if let Some((
            name,
            BehaviorTaskActionPhase::SupportedPrefix {
                first_unsupported_action,
                ..
            }
            | BehaviorTaskActionPhase::Unsupported {
                first_unsupported_action,
            },
        )) = current_phase
        {
            // Name the enclosing task too when the unsupported action is in a set.
            let enclosing_task = behavior_task
                .stack
                .first()
                .map(|frame| frame.program)
                .unwrap_or(behavior_task.program);
            warn!(actor = ?actor_entity, program = ?behavior_task.program, name,
                phase = ?behavior_task.phase, action = first_unsupported_action,
                ?enclosing_task,
                "behavior reached an unsupported authored action; failing its task");
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                actor_entity, &mut commands,
            );
            continue;
        }
        if let Some(return_frame) = behavior_task.stack.pop() {
            behavior_task.program = return_frame.program;
            behavior_task.document = return_frame.document;
            behavior_task.declaration = return_frame.declaration;
            behavior_task.phase = return_frame.phase;
            behavior_task.action = return_frame.action;
            behavior_task.repetitions = return_frame.repetitions;
            behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
            continue;
        }
        if behavior_task.phase == BehaviorTaskExecutionPhase::Execution
            && behavior_document_asset
                .behavior_task_at_index(behavior_task.declaration)
                .is_some_and(|task| !task.completion.is_empty())
        {
            behavior_task.phase = BehaviorTaskExecutionPhase::Completion;
            behavior_task.action = 0;
            behavior_task.repetitions = 0;
            behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
            continue;
        }
        if behavior_task.phase == BehaviorTaskExecutionPhase::Failure {
            failed_behavior_tasks.write(BehaviorTaskFailed {
                actor: actor_entity,
                execution_id: behavior_task.execution_id,
            });
        } else {
            finished_behavior_tasks.write(BehaviorTaskFinished {
                actor: actor_entity,
                execution_id: behavior_task.execution_id,
            });
        }
        commands.entity(actor_entity).remove::<(
            BehaviorTaskExecutionState,
            openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore,
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion,
        )>();
    }
}
