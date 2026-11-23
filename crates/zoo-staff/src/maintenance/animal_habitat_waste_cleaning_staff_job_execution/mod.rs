use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorTermination, RunningBehaviorInstruction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorKilledTarget;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_job_types::StaffJobBehaviorTask;
use openzt2_zoo_state::staff::staff_job_types::StaffJobCancellationPending;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCompleted;

use super::{
    maintenance_staff_job_claim_effect_progress_and_completion::create_completed_staff_job_message,
    maintenance_types::{AnimalHabitatWaste, AppliedHabitatWasteCleanup},
};

pub(super) fn apply_authored_staff_habitat_waste_termination(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut staff: Query<
        (
            Entity,
            &CurrentJob,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorTermination,
            ),
        ),
        (With<Staff>, Without<PendingBehaviorTaskFailure>),
    >,
    jobs: Query<(
        &StaffJob,
        &JobClaim,
        &StaffJobBehaviorTask,
        Option<&AppliedHabitatWasteCleanup>,
    )>,
    targets: Query<(&AnimalHabitatWaste, &DefinitionId, &GlobalTransform), With<WorldMember>>,
    cancelling: Query<(), With<StaffJobCancellationPending>>,
    mut consumed_targets: Local<HashSet<Entity>>,
) {
    consumed_targets.clear();
    for (actor, current, (execution, instruction, running_action)) in &mut staff {
        let Ok((job, claim, behavior, applied)) = jobs.get(current.job()) else {
            continue;
        };
        if job.kind != Some(StaffJobKind::CleanHabitat) || execution.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(execution) {
            continue;
        }
        let termination = &running_action.0;
        if !termination.kill_target || termination.kill_subject {
            continue;
        }
        if cancelling.contains(current.job())
            || job.token != Some(AssetId::from_key("t_rakepoo"))
            || claim.staff != actor
            || behavior.execution_id != Some(execution.execution_id)
            || execution.target != Some(job.target)
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if let Some(applied) = applied {
            // Failure branches can revisit the same kill after a later action
            // fails. Advance that instruction without consuming anything again.
            if applied.execution_id == execution.execution_id {
                instruction.complete(actor, clock.tick, &mut commands);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        }
        let Ok((waste, definition, transform)) = targets.get(job.target) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if waste.uncontained_waste_units == 0 || !consumed_targets.insert(job.target) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        commands.entity(job.target).despawn();
        commands.entity(actor).insert(BehaviorKilledTarget {
            execution_id: execution.execution_id,
            entity: job.target,
            definition: definition.0,
            translation: transform.translation(),
        });
        commands
            .entity(current.job())
            .insert(AppliedHabitatWasteCleanup {
                execution_id: execution.execution_id,
            });
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

pub(super) fn complete_applied_habitat_cleanup_after_behavior_exits(
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<(
        &StaffJob,
        &JobClaim,
        &StaffJobBehaviorTask,
        &AppliedHabitatWasteCleanup,
    )>,
    mut completed: MessageWriter<StaffJobCompleted>,
    cancelling: Query<(), With<StaffJobCancellationPending>>,
    mut settled_jobs: Local<HashSet<Entity>>,
) {
    settled_jobs.clear();
    // A later script failure cannot undo the preceding authored kill action.
    for (actor, execution_id) in finished
        .read()
        .map(|event| (event.actor, event.execution_id))
        .chain(failed.read().map(|event| (event.actor, event.execution_id)))
    {
        let Ok(current) = staff.get(actor) else {
            continue;
        };
        let Ok((job, claim, behavior, applied)) = jobs.get(current.job()) else {
            continue;
        };
        if job.kind == Some(StaffJobKind::CleanHabitat)
            && claim.staff == actor
            && behavior.execution_id == Some(execution_id)
            && applied.execution_id == execution_id
            && !cancelling.contains(current.job())
            && settled_jobs.insert(current.job())
        {
            completed.write(create_completed_staff_job_message(
                current.job(),
                job,
                actor,
                StaffJobKind::CleanHabitat,
            ));
        }
    }
}
