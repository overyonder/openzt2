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
        FacilityContainedWaste, LooseLitterSweepingStaffJobKindMarker,
        WasteContainerEmptyingStaffJobKindMarker,
    },
};

pub(super) fn advance_active_staff_waste_container_emptying_jobs(
    mut waste_container_emptying_jobs: Query<
        (Entity, &StaffJob, &JobClaim, &JobProgress),
        (
            With<WasteContainerEmptyingStaffJobKindMarker>,
            Without<LooseLitterSweepingStaffJobKindMarker>,
        ),
    >,
    staff_current_jobs: Query<&CurrentJob>,
    mut waste_container_targets: Query<&mut FacilityContainedWaste>,
    mut completed_staff_jobs: MessageWriter<StaffJobCompleted>,
    mut cancelled_staff_job_requests: MessageWriter<CancelStaffJobRequest>,
) {
    for (staff_job_entity, staff_job, job_claim, _) in &mut waste_container_emptying_jobs {
        if !staff_member_owns_active_job(
            staff_job_entity,
            staff_job,
            job_claim,
            StaffJobKind::EmptyBin,
            &staff_current_jobs,
        ) {
            continue;
        }
        let Ok(waste_container) = waste_container_targets.get_mut(staff_job.target) else {
            cancelled_staff_job_requests.write(CancelStaffJobRequest {
                job: staff_job_entity,
            });
            continue;
        };
        waste_container
            .map_unchanged(|waste| &mut waste.contained_waste_units)
            .set_if_neq(0);
        completed_staff_jobs.write(create_completed_staff_job_message(
            staff_job_entity,
            staff_job,
            job_claim.staff,
            StaffJobKind::EmptyBin,
        ));
    }
}
