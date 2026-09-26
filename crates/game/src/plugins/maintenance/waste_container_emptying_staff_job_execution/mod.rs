use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;

use crate::plugins::staff::{
    staff_job_types::{CurrentJob, JobClaim, JobProgress, StaffJob},
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
};

use super::{
    maintenance_staff_job_claim_effect_progress_and_completion::{
        create_completed_staff_job_message, staff_member_owns_active_job,
    },
    maintenance_types::{
        AnimalHabitatWasteCleaningStaffJobKindMarker, FacilityContainedWaste,
        IncrementalZooCleanlinessTotals, LooseLitterSweepingStaffJobKindMarker,
        MaintenanceRepairStaffJobKindMarker, WasteContainerEmptyingStaffJobKindMarker,
    },
};

pub(super) fn advance_active_staff_waste_container_emptying_jobs(
    mut waste_container_emptying_jobs: Query<
        (Entity, &StaffJob, &JobClaim, &JobProgress),
        (
            With<WasteContainerEmptyingStaffJobKindMarker>,
            Without<MaintenanceRepairStaffJobKindMarker>,
            Without<LooseLitterSweepingStaffJobKindMarker>,
            Without<AnimalHabitatWasteCleaningStaffJobKindMarker>,
        ),
    >,
    staff_current_jobs: Query<&CurrentJob>,
    mut waste_container_targets: Query<&mut FacilityContainedWaste>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
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
        let Ok(mut waste_container) = waste_container_targets.get_mut(staff_job.target) else {
            cancelled_staff_job_requests.write(CancelStaffJobRequest {
                job: staff_job_entity,
            });
            continue;
        };
        let previous_waste_container = *waste_container;
        waste_container.contained_waste_units = 0;
        cleanliness_totals
            .replace_facility_contained_waste(previous_waste_container, *waste_container);
        completed_staff_jobs.write(create_completed_staff_job_message(
            staff_job_entity,
            staff_job,
            job_claim.staff,
        ));
    }
}
