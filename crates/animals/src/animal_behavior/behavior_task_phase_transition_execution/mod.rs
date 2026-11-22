//! Return-stack, failure, completion and final-outcome transitions for behavior tasks.

use bevy::prelude::*;

#[cfg(test)]
mod tests;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy;
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::{
    BehaviorTargetRetryClock, BehaviorTargetRetryOutcomes,
};
#[cfg(test)]
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::{Dead, Frozen, Tranquilized};
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;

use openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    continue_behavior_program, BehaviorProgramBoundary, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

pub(super) fn begin_behavior_target_candidate_selection_updates(
    mut actors: Query<&mut BehaviorTargetRetryClock>,
) {
    for mut retry_delays in &mut actors {
        retry_delays.begin_candidate_selection_update();
    }
}

pub(super) fn advance_behavior_target_retry_clocks(
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut actors: Query<
        (
            &mut BehaviorTargetRetryClock,
            Option<&BehaviorTaskExecutionState>,
            Option<&RunningBehaviorInstruction>,
        ),
        (
            With<WorldMember>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorSet>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
) {
    if !definitions
        .get(&assets)
        .and_then(|view| view.behavior_selection_policy())
        .is_some_and(|policy| policy.enabled)
    {
        return;
    }
    for (mut retry, task, instruction) in &mut actors {
        if task
            .is_none_or(|task| instruction.is_some_and(|instruction| instruction.is_current(task)))
        {
            retry.advance_cognitive_update();
        }
    }
}

pub(super) fn advance_behavior_tasks_through_return_failure_completion_and_final_outcome(
    boundary: On<BehaviorProgramBoundary>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut behavior_tasks: Query<(
        Entity,
        &mut BehaviorTaskExecutionState,
        Option<&PendingBehaviorTaskFailure>,
        Option<&RunningBehaviorInstruction>,
        Option<&BehaviorTargetRetryClock>,
    )>,
    target_definitions: Query<&DefinitionId>,
    continuations: BehaviorContinuationFrames,
    mut commands: Commands,
    mut retry_outcomes: BehaviorTargetRetryOutcomes,
    mut failed_behavior_tasks: MessageWriter<BehaviorTaskFailed>,
    mut finished_behavior_tasks: MessageWriter<BehaviorTaskFinished>,
) {
    let retry_policy = world_definitions
        .get(&world_definition_assets)
        .and_then(|definitions| definitions.behavior_selection_policy().copied());
    let actor_entity = boundary.entity;
    let tick = boundary.tick;
    let Ok((_, mut behavior_task, pending_failure, instruction, retry_delays)) =
        behavior_tasks.get_mut(actor_entity)
    else {
        return;
    };
    let retry_target = behavior_task
        .target()
        .filter(|target| target_definitions.contains(*target));
    if pending_failure.is_none() && behavior_task.next_action_tick > tick {
        return;
    }
    let failed = if pending_failure.is_some() {
        if behavior_task.phase == BehaviorTaskExecutionPhase::Set {
            if let Some(frame) = continuations.pop(actor_entity, &mut commands) {
                let depth = behavior_task.depth.saturating_sub(1);
                behavior_task.restore_suspended_instruction(&frame, depth, tick);
                commands.trigger(BehaviorProgramBoundary {
                    entity: actor_entity,
                    tick,
                });
                return;
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
            continuations.truncate(actor_entity, 0, &mut commands);
            behavior_task.depth = 0;
            behavior_task.next_action_tick = tick;
            commands
                .entity(actor_entity)
                .remove::<PendingBehaviorTaskFailure>();
            continue_behavior_program(actor_entity, tick, &mut commands);
            return;
        }
        true
    } else {
        let Some(document) = behavior_document_assets.get(&behavior_task.document) else {
            return;
        };
        if instruction.is_some_and(|instruction| instruction.is_current(&behavior_task)) {
            return;
        }
        if let Some(frame) = continuations.pop(actor_entity, &mut commands) {
            let depth = behavior_task.depth.saturating_sub(1);
            behavior_task.restore_suspended_instruction(&frame, depth, tick);
            continue_behavior_program(actor_entity, tick, &mut commands);
            return;
        }
        if behavior_task.phase == BehaviorTaskExecutionPhase::Execution
            && document
                .behavior_task_at_index(behavior_task.declaration)
                .is_some_and(|task| !task.completion.is_empty())
        {
            behavior_task.phase = BehaviorTaskExecutionPhase::Completion;
            behavior_task.action = 0;
            behavior_task.repetitions = 0;
            behavior_task.next_action_tick = tick;
            continue_behavior_program(actor_entity, tick, &mut commands);
            return;
        }
        behavior_task.phase == BehaviorTaskExecutionPhase::Failure
    };
    finish_target_retry_outcome(
        actor_entity,
        retry_target,
        failed,
        retry_policy,
        retry_delays,
        &mut retry_outcomes,
        &mut commands,
    );
    if failed {
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
        PendingBehaviorTaskFailure,
        PendingBehaviorAnimationClipCompletion,
        PendingBehaviorDockingCompletion,
        openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion,
    )>();
}

fn finish_target_retry_outcome(
    actor: Entity,
    target: Option<Entity>,
    failed: bool,
    policy: Option<BehaviorSelectionPolicy>,
    clock: Option<&BehaviorTargetRetryClock>,
    outcomes: &mut BehaviorTargetRetryOutcomes,
    commands: &mut Commands,
) {
    let Some(target) = target.filter(|target| *target != actor) else {
        return;
    };
    if failed && policy.is_none() {
        return;
    }
    outcomes.settle(commands, actor, target, clock, policy.filter(|_| failed));
}
