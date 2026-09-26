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
        AnimalHabitatWasteCleaningStaffJobKindMarker, IncrementalZooCleanlinessTotals,
        LooseLitterSweepingStaffJobKindMarker, LooseLitterWaste,
        MaintenanceRepairStaffJobKindMarker, WasteContainerEmptyingStaffJobKindMarker,
    },
};

pub(super) fn advance_active_staff_litter_sweeping_jobs(
    mut commands: Commands,
    mut litter_sweeping_jobs: Query<
        (Entity, &StaffJob, &JobClaim, &JobProgress),
        (
            With<LooseLitterSweepingStaffJobKindMarker>,
            Without<MaintenanceRepairStaffJobKindMarker>,
            Without<WasteContainerEmptyingStaffJobKindMarker>,
            Without<AnimalHabitatWasteCleaningStaffJobKindMarker>,
        ),
    >,
    staff_current_jobs: Query<&CurrentJob>,
    mut litter_targets: Query<&mut LooseLitterWaste>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
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
        let Ok(mut loose_litter_waste) = litter_targets.get_mut(staff_job.target) else {
            cancelled_staff_job_requests.write(CancelStaffJobRequest {
                job: staff_job_entity,
            });
            continue;
        };
        let previous_litter_units = loose_litter_waste.uncontained_waste_units;
        loose_litter_waste.uncontained_waste_units = 0;
        cleanliness_totals.replace_uncontained_waste_units(
            previous_litter_units,
            loose_litter_waste.uncontained_waste_units,
        );
        let litter_is_exhausted = loose_litter_waste.uncontained_waste_units == 0;
        drop(loose_litter_waste);
        if litter_is_exhausted {
            commands.entity(staff_job.target).despawn();
        }
        completed_staff_jobs.write(create_completed_staff_job_message(
            staff_job_entity,
            staff_job,
            job_claim.staff,
        ));
    }
}
