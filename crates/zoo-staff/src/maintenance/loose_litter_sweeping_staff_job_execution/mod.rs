use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;

use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::JobProgress;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCompleted;

use super::{
    maintenance_staff_job_claim_effect_progress_and_completion::{
        create_completed_staff_job_message, staff_member_owns_active_job,
    },
    maintenance_types::{
        LooseLitterSweepingStaffJobKindMarker, LooseLitterWaste,
        WasteContainerEmptyingStaffJobKindMarker,
    },
};

pub(super) fn advance_active_staff_litter_sweeping_jobs(
    mut commands: Commands,
    mut litter_sweeping_jobs: Query<
        (Entity, &StaffJob, &JobClaim, &JobProgress),
        (
            With<LooseLitterSweepingStaffJobKindMarker>,
            Without<WasteContainerEmptyingStaffJobKindMarker>,
        ),
    >,
    staff_current_jobs: Query<&CurrentJob>,
    litter_targets: Query<(), With<LooseLitterWaste>>,
    mut completed_staff_jobs: MessageWriter<StaffJobCompleted>,
    mut cancelled_staff_job_requests: MessageWriter<CancelStaffJobRequest>,
) {
    for (staff_job_entity, staff_job, job_claim, _) in &mut litter_sweeping_jobs {
        if !staff_member_owns_active_job(
            staff_job_entity,
            staff_job,
            job_claim,
            StaffJobKind::SweepLitter,
            &staff_current_jobs,
        ) {
            continue;
        }
        if !litter_targets.contains(staff_job.target) {
            cancelled_staff_job_requests.write(CancelStaffJobRequest {
                job: staff_job_entity,
            });
            continue;
        }
        commands.entity(staff_job.target).try_despawn();
        completed_staff_jobs.write(create_completed_staff_job_message(
            staff_job_entity,
            staff_job,
            job_claim.staff,
        ));
    }
}
