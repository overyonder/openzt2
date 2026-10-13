use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::{
    behavior::action_record::BehaviorAction, world_definitions::staff_management::StaffJobKind,
    AssetId,
};

use crate::plugins::{
    animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, BehaviorTaskFailed, BehaviorTaskFinished,
        PendingBehaviorTaskFailure,
    },
    simulation_time::simulation_clock_types::ZooClock,
    staff::{
        staff_employment_types::Staff,
        staff_job_types::{CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask},
        staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
    },
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

use super::{
    maintenance_staff_job_claim_effect_progress_and_completion::create_completed_staff_job_message,
    maintenance_types::{AnimalHabitatWaste, AppliedHabitatWasteCleanup},
};

pub(super) fn apply_authored_staff_habitat_waste_termination(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    mut staff: Query<
        (Entity, &CurrentJob, &mut BehaviorTaskExecutionState),
        (With<Staff>, Without<PendingBehaviorTaskFailure>),
    >,
    jobs: Query<(
        &StaffJob,
        &JobClaim,
        &StaffJobBehaviorTask,
        Option<&AppliedHabitatWasteCleanup>,
    )>,
    targets: Query<&AnimalHabitatWaste, With<WorldMember>>,
    mut cancellations: MessageReader<CancelStaffJobRequest>,
    mut cancelled_jobs: Local<HashSet<Entity>>,
    mut consumed_targets: Local<HashSet<Entity>>,
) {
    cancelled_jobs.clear();
    consumed_targets.clear();
    cancelled_jobs.extend(cancellations.read().map(|request| request.job));
    for (actor, current, mut execution) in &mut staff {
        let Ok((job, claim, behavior, applied)) = jobs.get(current.job) else {
            continue;
        };
        if job.kind != StaffJobKind::CleanHabitat || execution.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::Termination(termination)) = documents
            .get(&execution.document)
            .and_then(|document| find_current_behavior_task_action(document, &execution))
        else {
            continue;
        };
        if !termination.kill_target || termination.kill_subject {
            continue;
        }
        if cancelled_jobs.contains(&current.job)
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
                advance_behavior_task_to_next_action(&mut execution, clock.tick);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        }
        let Ok(waste) = targets.get(job.target) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if waste.uncontained_waste_units == 0 || !consumed_targets.insert(job.target) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        // The component removal hook owns the cleanliness debit.
        commands.entity(job.target).despawn();
        commands
            .entity(current.job)
            .insert(AppliedHabitatWasteCleanup {
                execution_id: execution.execution_id,
            });
        advance_behavior_task_to_next_action(&mut execution, clock.tick);
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
    mut cancellations: MessageReader<CancelStaffJobRequest>,
    mut settled_jobs: Local<HashSet<Entity>>,
) {
    settled_jobs.clear();
    settled_jobs.extend(cancellations.read().map(|request| request.job));
    // A later script failure cannot undo the preceding authored kill action.
    for (actor, execution_id) in finished
        .read()
        .map(|event| (event.actor, event.execution_id))
        .chain(failed.read().map(|event| (event.actor, event.execution_id)))
    {
        let Ok(current) = staff.get(actor) else {
            continue;
        };
        let Ok((job, claim, behavior, applied)) = jobs.get(current.job) else {
            continue;
        };
        if job.kind == StaffJobKind::CleanHabitat
            && claim.staff == actor
            && behavior.execution_id == Some(execution_id)
            && applied.execution_id == execution_id
            && settled_jobs.insert(current.job)
        {
            completed.write(create_completed_staff_job_message(current.job, job, actor));
        }
    }
}
