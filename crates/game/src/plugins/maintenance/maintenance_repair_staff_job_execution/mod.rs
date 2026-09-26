use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;

use crate::plugins::{
    staff::{
        staff_job_types::{CurrentJob, JobClaim, JobProgress, StaffJob},
        staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
    },
    world_spawn::world_membership_types::DefinitionId,
};

use super::{
    maintenance_staff_job_claim_effect_progress_and_completion::{
        create_completed_staff_job_message, staff_member_owns_active_job,
    },
    maintenance_types::{
        AnimalHabitatWasteCleaningStaffJobKindMarker, IncrementalZooCleanlinessTotals,
        LooseLitterSweepingStaffJobKindMarker, MaintainableObjectConditionPermille,
        MaintenanceRepairStaffJobKindMarker, WasteContainerEmptyingStaffJobKindMarker,
    },
};

pub(super) fn advance_active_staff_repair_jobs(
    mut repair_jobs: Query<
        (Entity, &StaffJob, &JobClaim, &JobProgress),
        (
            With<MaintenanceRepairStaffJobKindMarker>,
            Without<WasteContainerEmptyingStaffJobKindMarker>,
            Without<LooseLitterSweepingStaffJobKindMarker>,
            Without<AnimalHabitatWasteCleaningStaffJobKindMarker>,
        ),
    >,
    staff_current_jobs: Query<&CurrentJob>,
    mut repair_targets: Query<(&DefinitionId, &mut MaintainableObjectConditionPermille)>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
    mut completed_staff_jobs: MessageWriter<StaffJobCompleted>,
    mut cancelled_staff_job_requests: MessageWriter<CancelStaffJobRequest>,
) {
    for (staff_job_entity, staff_job, job_claim, _) in &mut repair_jobs {
        if !staff_member_owns_active_job(
            staff_job_entity,
            staff_job,
            job_claim,
            StaffJobKind::Repair,
            &staff_current_jobs,
        ) {
            continue;
        }
        let Ok((_, mut maintainable_object_condition)) = repair_targets.get_mut(staff_job.target)
        else {
            cancelled_staff_job_requests.write(CancelStaffJobRequest {
                job: staff_job_entity,
            });
            continue;
        };
        let previous_condition = *maintainable_object_condition;
        maintainable_object_condition.0 = 1_000;
        cleanliness_totals.replace_maintainable_object_condition(
            previous_condition,
            *maintainable_object_condition,
        );
        completed_staff_jobs.write(create_completed_staff_job_message(
            staff_job_entity,
            staff_job,
            job_claim.staff,
        ));
    }
}
