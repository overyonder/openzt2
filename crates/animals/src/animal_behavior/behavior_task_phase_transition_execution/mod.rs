//! Return-stack, failure, completion and final-outcome transitions for behavior tasks.

use bevy::prelude::*;

#[cfg(test)]
mod tests;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::behavior::document::BehaviorTaskActionPhase;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy;
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRetryDelays;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::{Dead, Frozen, Tranquilized};
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;

use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

pub(super) fn begin_behavior_target_candidate_selection_updates(
    mut actors: Query<&mut BehaviorTargetRetryDelays>,
) {
    for mut retry_delays in &mut actors {
        retry_delays.begin_candidate_selection_update();
    }
}

pub(super) fn advance_behavior_tasks_through_return_failure_completion_and_final_outcome(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut behavior_tasks: Query<(
        Entity,
        &mut BehaviorTaskExecutionState,
        Option<&PendingBehaviorTaskFailure>,
        Option<&mut BehaviorTargetRetryDelays>,
    )>,
    entities: Query<()>,
    target_definitions: Query<&DefinitionId>,
    thinking_actors: Query<
        (),
        (
            With<WorldMember>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorSet>,
        ),
    >,
    mut idle_retry_delays: Query<
        (Entity, &mut BehaviorTargetRetryDelays),
        Without<BehaviorTaskExecutionState>,
    >,
    mut commands: Commands,
    simulation_clock: Res<ZooClock>,
    mut failed_behavior_tasks: MessageWriter<BehaviorTaskFailed>,
    mut finished_behavior_tasks: MessageWriter<BehaviorTaskFinished>,
) {
    let retry_policy = world_definitions
        .get(&world_definition_assets)
        .and_then(|definitions| definitions.behavior_selection_policy().copied());
    let thinking_enabled = retry_policy.is_some_and(|policy| policy.enabled);
    // An idle thinker still updates when all its current candidates are rejected.
    // No task is created just to keep its rejection counters moving.
    for (actor, mut retry_delays) in &mut idle_retry_delays {
        retry_delays.remove_destroyed_targets(&entities);
        if thinking_enabled && thinking_actors.contains(actor) {
            retry_delays.advance_cognitive_update();
        }
    }
    for (actor_entity, mut behavior_task, pending_failure, mut retry_delays) in &mut behavior_tasks
    {
        // Region and terrain-point stand-ins are not individual target entities.
        let retry_target = behavior_task
            .target()
            .filter(|target| target_definitions.contains(*target));
        if let Some(retry_delays) = retry_delays.as_deref_mut() {
            retry_delays.remove_destroyed_targets(&entities);
            if thinking_enabled
                && thinking_actors.contains(actor_entity)
                && pending_failure.is_none()
                && behavior_document_assets
                    .get(&behavior_task.document)
                    .and_then(|document| {
                        find_current_behavior_task_action(document, &behavior_task)
                    })
                    .is_some()
            {
                retry_delays.advance_cognitive_update();
            }
        }
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
                finish_target_retry_outcome(
                    actor_entity,
                    retry_target,
                    true,
                    retry_policy,
                    retry_delays.as_deref_mut(),
                    &mut commands,
                );
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
            finish_target_retry_outcome(
                actor_entity,
                retry_target,
                true,
                retry_policy,
                retry_delays.as_deref_mut(),
                &mut commands,
            );
            failed_behavior_tasks.write(BehaviorTaskFailed {
                actor: actor_entity,
                execution_id: behavior_task.execution_id,
            });
        } else {
            finish_target_retry_outcome(
                actor_entity,
                retry_target,
                false,
                retry_policy,
                retry_delays.as_deref_mut(),
                &mut commands,
            );
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

fn finish_target_retry_outcome(
    actor: Entity,
    target: Option<Entity>,
    failed: bool,
    policy: Option<BehaviorSelectionPolicy>,
    retry_delays: Option<&mut BehaviorTargetRetryDelays>,
    commands: &mut Commands,
) {
    let Some(target) = target.filter(|target| *target != actor) else {
        return;
    };
    if let Some(retry_delays) = retry_delays {
        if let Some(policy) = policy.filter(|_| failed) {
            retry_delays.record_failure(target, policy);
        } else if !failed {
            retry_delays.clear_successful_target(target);
        }
    } else if let Some(policy) = policy.filter(|_| failed) {
        let mut retry_delays = BehaviorTargetRetryDelays::default();
        retry_delays.record_failure(target, policy);
        commands.entity(actor).insert(retry_delays);
    }
}
