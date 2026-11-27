use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;

use super::{
    staff_employment_types::Staff,
    staff_job_types::{
        CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask, StaffJobCancellationPending,
    },
    staff_lifecycle_messages::StaffJobCompleted,
};

pub(super) fn acknowledge_staff_job_completion_fact_actions_owned_by_terminal_domain_systems(
    mut finished: MessageReader<BehaviorTaskFinished>,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<
        (&StaffJob, &JobClaim, &StaffJobBehaviorTask),
        Without<StaffJobCancellationPending>,
    >,
    mut completed: MessageWriter<StaffJobCompleted>,
) {
    for outcome in finished.read() {
        let Ok(current) = staff.get(outcome.actor) else {
            continue;
        };
        let Ok((job, claim, behavior)) = jobs.get(current.job()) else {
            continue;
        };
        if job.kind != Some(StaffJobKind::EmptyBin)
            || claim.staff != outcome.actor
            || behavior.execution_id != Some(outcome.execution_id)
        {
            continue;
        }
        let Some(target) = job.target else { continue };
        completed.write(StaffJobCompleted {
            staff: outcome.actor,
            job: current.job(),
            kind: StaffJobKind::EmptyBin,
            target,
        });
    }
}
