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
        AnimalHabitatWaste, AnimalHabitatWasteCleaningStaffJobKindMarker,
        IncrementalZooCleanlinessTotals, LooseLitterSweepingStaffJobKindMarker,
        MaintenanceRepairStaffJobKindMarker, WasteContainerEmptyingStaffJobKindMarker,
    },
};

pub(super) fn advance_active_staff_habitat_waste_cleaning_jobs(
    mut commands: Commands,
    mut habitat_waste_cleaning_jobs: Query<
        (Entity, &StaffJob, &JobClaim, &JobProgress),
        (
            With<AnimalHabitatWasteCleaningStaffJobKindMarker>,
            Without<MaintenanceRepairStaffJobKindMarker>,
            Without<WasteContainerEmptyingStaffJobKindMarker>,
            Without<LooseLitterSweepingStaffJobKindMarker>,
        ),
    >,
    staff_current_jobs: Query<&CurrentJob>,
    mut habitat_waste_targets: Query<&mut AnimalHabitatWaste>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
    mut completed_staff_jobs: MessageWriter<StaffJobCompleted>,
    mut cancelled_staff_job_requests: MessageWriter<CancelStaffJobRequest>,
) {
    for (staff_job_entity, staff_job, job_claim, _) in &mut habitat_waste_cleaning_jobs {
        if !staff_member_owns_active_job(
            staff_job_entity,
            staff_job,
            job_claim,
            StaffJobKind::CleanHabitat,
            &staff_current_jobs,
        ) {
            continue;
        }
        let Ok(mut habitat_waste) = habitat_waste_targets.get_mut(staff_job.target) else {
            cancelled_staff_job_requests.write(CancelStaffJobRequest {
                job: staff_job_entity,
            });
            continue;
        };
        let previous_habitat_waste_units = habitat_waste.uncontained_waste_units;
        habitat_waste.uncontained_waste_units = 0;
        cleanliness_totals.replace_uncontained_waste_units(
            previous_habitat_waste_units,
            habitat_waste.uncontained_waste_units,
        );
        let habitat_waste_is_exhausted = habitat_waste.uncontained_waste_units == 0;
        drop(habitat_waste);
        if habitat_waste_is_exhausted {
            commands.entity(staff_job.target).despawn();
        }
        completed_staff_jobs.write(create_completed_staff_job_message(
            staff_job_entity,
            staff_job,
            job_claim.staff,
        ));
    }
}
