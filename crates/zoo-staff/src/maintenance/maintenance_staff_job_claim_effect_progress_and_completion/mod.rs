use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;

use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCompleted;

pub(super) fn staff_member_owns_active_job(
    staff_job_entity: Entity,
    staff_job: &StaffJob,
    job_claim: &JobClaim,
    required_job_kind: StaffJobKind,
    staff_current_jobs: &Query<&CurrentJob>,
) -> bool {
    staff_job.kind == required_job_kind
        && staff_current_jobs
            .get(job_claim.staff)
            .is_ok_and(|current_job| current_job.job == staff_job_entity)
}

pub(super) fn create_completed_staff_job_message(
    staff_job_entity: Entity,
    staff_job: &StaffJob,
    staff_member_entity: Entity,
) -> StaffJobCompleted {
    StaffJobCompleted {
        staff: staff_member_entity,
        job: staff_job_entity,
        kind: staff_job.kind,
        target: staff_job.target,
    }
}
