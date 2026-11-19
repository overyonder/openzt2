use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;

use super::{
    staff_employment_types::Staff,
    staff_job_types::{CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask},
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
};

pub(super) fn apply_completed_tank_maintenance_work(
    mut finished: MessageReader<BehaviorTaskFinished>,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<(&StaffJob, &JobClaim, &StaffJobBehaviorTask)>,
    cancellations: Res<Messages<CancelStaffJobRequest>>,
    mut completed: MessageWriter<StaffJobCompleted>,
) {
    for outcome in finished.read() {
        let Ok(current) = staff.get(outcome.actor) else {
            continue;
        };
        let Ok((job, claim, behavior)) = jobs.get(current.job) else {
            continue;
        };
        if job.kind != StaffJobKind::MaintainTank
            || claim.staff != outcome.actor
            || behavior.execution_id != Some(outcome.execution_id)
            || cancellations
                .get_cursor()
                .read(&cancellations)
                .any(|event| event.job == current.job)
        {
            continue;
        }
        completed.write(StaffJobCompleted {
            staff: outcome.actor,
            job: current.job,
            kind: job.kind,
            target: job.target,
        });
    }
}
