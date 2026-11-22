use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::behavior_task_execution_types::abandon_behavior_task_for_reconsideration;
use openzt2_zoo_state::maintenance::maintenance_types::AppliedHabitatWasteCleanup;

use super::{
    staff_job_types::{JobClaim, StaffJob, StaffJobBehaviorTask, StaffJobCancellationPending},
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCancelled, StaffJobCompleted},
};
use openzt2_zoo_state::behavior_task_execution_types::AppliedBehaviorLitterKill;

pub(super) fn mark_explicit_staff_job_cancellations(
    mut commands: Commands,
    mut requests: MessageReader<CancelStaffJobRequest>,
    jobs: Query<(), With<StaffJob>>,
) {
    for request in requests.read() {
        if jobs.contains(request.job) {
            commands
                .entity(request.job)
                .insert_if_new(StaffJobCancellationPending);
        }
    }
}

pub(crate) fn release_staff_and_despawn_completed_jobs(
    mut commands: Commands,
    mut completed_jobs: MessageReader<StaffJobCompleted>,
    claims: Query<&JobClaim, Without<StaffJobCancellationPending>>,
) {
    for completed_job in completed_jobs.read() {
        if claims
            .get(completed_job.job)
            .map_or(true, |claim| claim.staff != completed_job.staff)
        {
            continue;
        }
        commands.entity(completed_job.job).despawn();
    }
}

pub fn cancel_requested_jobs_and_jobs_with_invalid_world_relations(
    mut commands: Commands,
    mut cancellation_requests: MessageReader<CancelStaffJobRequest>,
    jobs: Query<(
        Entity,
        &StaffJob,
        Option<&JobClaim>,
        Option<&StaffJobBehaviorTask>,
        Option<&AppliedHabitatWasteCleanup>,
        Option<&AppliedBehaviorLitterKill>,
        Has<StaffJobCancellationPending>,
    )>,
    world_members: Query<(), With<WorldMember>>,
    dead_animals: Query<(), With<Dead>>,
    mut cancelled_jobs: MessageWriter<StaffJobCancelled>,
    mut explicitly_requested_jobs: Local<HashSet<Entity>>,
) {
    explicitly_requested_jobs.clear();
    explicitly_requested_jobs.extend(cancellation_requests.read().map(|request| request.job));
    for (job_entity, job, claim, behavior, applied_cleanup, applied_litter, cancelling) in &jobs {
        let target_is_live = world_members.get(job.target).is_ok()
            && !(job.kind == Some(StaffJobKind::Treat) && dead_animals.contains(job.target));
        let target_was_consumed = claim.is_some()
            && behavior.is_some_and(|behavior| match job.kind {
                Some(StaffJobKind::CleanHabitat) => applied_cleanup
                    .is_some_and(|applied| behavior.execution_id == Some(applied.execution_id)),
                Some(StaffJobKind::SweepLitter) => {
                    applied_litter.is_some_and(|applied| behavior.execution_id == Some(applied.0))
                }
                _ => false,
            });
        if (target_is_live || target_was_consumed)
            && !cancelling
            && !explicitly_requested_jobs.contains(&job_entity)
        {
            continue;
        }
        if let Some(kind) = job.kind.filter(|_| target_is_live) {
            cancelled_jobs.write(StaffJobCancelled {
                kind,
                target: job.target,
                token: job.token,
            });
        }
        if let Some(claim) = claim {
            abandon_behavior_task_for_reconsideration(claim.staff, &mut commands);
        }
        commands.entity(job_entity).try_despawn();
    }
}
