use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::{
    behavior::{
        action::modification::{BehaviorFact, BehaviorFactModificationOperation},
        action_record::BehaviorAction,
    },
    world_definitions::staff_management::StaffJobKind,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;

use super::{
    staff_job_types::{
        JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask, TankCleaningRequestSite,
    },
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
};

/// Applies the claimed task's own water-dirtiness change to its tank once
/// the task finishes: `ZTBehChangeWaterDirtiness Clean="true"` leaves the
/// water without dirt. A task that changes dirtiness any other way has no
/// supported effect, so its job is withdrawn rather than completed.
pub(crate) fn apply_completed_tank_maintenance_work(
    documents: Res<Assets<BehaviorDocumentAsset>>,
    jobs: Query<(Entity, &StaffJob, &JobClaim, &StaffJobBehaviorTask), With<JobProgress>>,
    sites: Query<&TankCleaningRequestSite>,
    mut tank_water_quality: Query<&mut WaterQuality>,
    mut completed_jobs: MessageWriter<StaffJobCompleted>,
    mut cancellations: ParamSet<(
        MessageReader<CancelStaffJobRequest>,
        MessageWriter<CancelStaffJobRequest>,
    )>,
    mut cancelled_jobs: Local<HashSet<Entity>>,
) {
    cancelled_jobs.clear();
    cancelled_jobs.extend(cancellations.p0().read().map(|request| request.job));
    for (job_entity, job, claim, behavior_task) in &jobs {
        // Cancellation is settled in Cleanup, after this terminal effect in Act.
        if job.kind != StaffJobKind::MaintainTank || cancelled_jobs.contains(&job_entity) {
            continue;
        }
        let Some(operation) = documents
            .get(&behavior_task.document)
            .and_then(|document| document.behavior_task_at_index(behavior_task.declaration_index))
            .filter(|task| task.id == behavior_task.task)
            .and_then(|task| {
                task.execution
                    .lowered_actions()
                    .iter()
                    .filter_map(|action| match action {
                        BehaviorAction::FactModifications(modifications) => Some(modifications),
                        _ => None,
                    })
                    .flatten()
                    .find(|modification| modification.modified_fact == BehaviorFact::WaterDirtiness)
                    .map(|modification| modification.modification_operation)
            })
        else {
            continue;
        };
        // A keeper cleans the tank the manager's request site lies in.
        let tank = sites.get(job.target).map_or(job.target, |site| site.tank);
        let Ok(mut water_quality) = tank_water_quality.get_mut(tank) else {
            continue;
        };
        if operation != BehaviorFactModificationOperation::Clear {
            cancellations
                .p1()
                .write(CancelStaffJobRequest { job: job_entity });
            continue;
        }
        water_quality.0 = 1000;
        completed_jobs.write(StaffJobCompleted {
            staff: claim.staff,
            job: job_entity,
            kind: job.kind,
            target: job.target,
        });
    }
}
