//! Return-stack, failure, completion and final-outcome transitions for behavior tasks.

use bevy::prelude::*;

#[cfg(test)]
mod tests;

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::simulation_time::simulation_clock_types::ZooClock,
};

use crate::plugins::behavior_task_execution_types::{
    find_current_behavior_task_action, BehaviorTaskExecutionPhase, BehaviorTaskExecutionState,
    BehaviorTaskFailed, BehaviorTaskFinished, PendingBehaviorAnimationClipCompletion,
    PendingBehaviorDockingCompletion, PendingBehaviorTaskFailure,
};

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
                    PendingBehaviorTaskFailure,
                    PendingBehaviorAnimationClipCompletion,
                    PendingBehaviorDockingCompletion,
                    crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
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
        let unsupported_suffix = if behavior_task.phase == BehaviorTaskExecutionPhase::Set {
            behavior_document_asset
                .behavior_set_at_index(behavior_task.declaration)
                .is_some_and(|set| set.actions.has_unsupported_suffix())
        } else {
            behavior_document_asset
                .behavior_task_at_index(behavior_task.declaration)
                .is_some_and(|task| match behavior_task.phase {
                    BehaviorTaskExecutionPhase::Execution => {
                        task.execution.has_unsupported_suffix()
                    }
                    BehaviorTaskExecutionPhase::Completion => {
                        task.completion.has_unsupported_suffix()
                    }
                    BehaviorTaskExecutionPhase::Failure => task.failure.has_unsupported_suffix(),
                    BehaviorTaskExecutionPhase::Set => false,
                })
        };
        if unsupported_suffix {
            warn!(actor = ?actor_entity, program = ?behavior_task.program,
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
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
        )>();
    }
}
