use bevy::{platform::collections::HashSet, prelude::*};

use crate::plugins::{
    animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
        PendingBehaviorDockingCompletion, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{Destination, Docking},
    world_spawn::world_membership_types::WorldMember,
};

use super::{
    staff_employment_types::{AvailableForWork, Staff},
    staff_job_types::{CurrentJob, JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask},
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCancelled, StaffJobCompleted},
};

pub(in crate::plugins::staff) fn release_staff_and_despawn_completed_jobs(
    mut commands: Commands,
    mut completed_jobs: MessageReader<StaffJobCompleted>,
    staff: Query<&CurrentJob, With<Staff>>,
    claims: Query<&JobClaim>,
) {
    for completed_job in completed_jobs.read() {
        if claims
            .get(completed_job.job)
            .map_or(true, |claim| claim.staff != completed_job.staff)
            || staff
                .get(completed_job.staff)
                .map_or(true, |current_job| current_job.job != completed_job.job)
        {
            continue;
        }
        commands.entity(completed_job.staff).remove::<CurrentJob>();
        commands
            .entity(completed_job.staff)
            .insert(AvailableForWork);
        commands.entity(completed_job.job).despawn();
    }
}

pub(crate) fn cancel_requested_jobs_and_jobs_with_invalid_world_relations(
    mut commands: Commands,
    mut cancellation_requests: MessageReader<CancelStaffJobRequest>,
    jobs: Query<(Entity, &StaffJob, Option<&JobClaim>)>,
    staff: Query<(Entity, Option<&CurrentJob>), With<Staff>>,
    world_members: Query<(), With<WorldMember>>,
    mut cancelled_jobs: MessageWriter<StaffJobCancelled>,
    mut explicitly_requested_jobs: Local<HashSet<Entity>>,
) {
    explicitly_requested_jobs.clear();
    explicitly_requested_jobs.extend(cancellation_requests.read().map(|request| request.job));
    for (job_entity, job, claim) in &jobs {
        let target_is_live = world_members.get(job.target).is_ok();
        let claimed_staff_is_valid = claim.is_none_or(|claim| {
            staff.get(claim.staff).is_ok_and(|(_, current_job)| {
                current_job.is_some_and(|current_job| current_job.job == job_entity)
            })
        });
        if target_is_live
            && !claimed_staff_is_valid
            && !explicitly_requested_jobs.contains(&job_entity)
        {
            commands
                .entity(job_entity)
                .remove::<(JobClaim, StaffJobBehaviorTask, JobProgress)>();
            continue;
        }
        if target_is_live
            && claimed_staff_is_valid
            && !explicitly_requested_jobs.contains(&job_entity)
        {
            continue;
        }
        if target_is_live {
            cancelled_jobs.write(StaffJobCancelled {
                kind: job.kind,
                target: job.target,
                token: job.token,
            });
        }
        if let Some(claim) = claim {
            if staff
                .get(claim.staff)
                .is_ok_and(|(_, current)| current.is_some_and(|current| current.job == job_entity))
            {
                commands.entity(claim.staff).remove::<(
                    CurrentJob,
                    BehaviorTaskExecutionState,
                    PendingBehaviorAnimationClipCompletion,
                    PendingBehaviorDockingCompletion,
                    PendingBehaviorMoveCompletion,
                    PendingBehaviorTaskFailure,
                    Destination,
                    Docking,
                )>();
                commands.entity(claim.staff).insert(AvailableForWork);
            }
        }
        commands.entity(job_entity).despawn();
    }
    for (staff_entity, current_job) in &staff {
        let Some(current_job) = current_job else {
            continue;
        };
        if jobs.get(current_job.job).is_ok() {
            continue;
        }
        commands.entity(staff_entity).remove::<(
            CurrentJob,
            BehaviorTaskExecutionState,
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            PendingBehaviorMoveCompletion,
            PendingBehaviorTaskFailure,
            Destination,
            Docking,
        )>();
        commands.entity(staff_entity).insert(AvailableForWork);
    }
}
