use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;

use super::{
    staff_job_types::{JobClaim, JobProgress, StaffJob},
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted, TankWaterCleaned},
};

pub(crate) fn apply_completed_tank_maintenance_work(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut jobs: Query<(Entity, &StaffJob, &JobProgress, &JobClaim)>,
    mut tank_water_quality: Query<&mut WaterQuality>,
    mut completed_jobs: MessageWriter<StaffJobCompleted>,
    mut cleaned_tank_water: MessageWriter<TankWaterCleaned>,
    mut cancellations: MessageReader<CancelStaffJobRequest>,
    mut cancelled_jobs: Local<HashSet<Entity>>,
) {
    cancelled_jobs.clear();
    cancelled_jobs.extend(cancellations.read().map(|request| request.job));
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (job_entity, job, _, claim) in &mut jobs {
        // Cancellation is settled in Cleanup, after this terminal effect in Act.
        if job.kind != StaffJobKind::MaintainTank || cancelled_jobs.contains(&job_entity) {
            continue;
        }
        let applied = match job.kind {
            StaffJobKind::MaintainTank => catalogue.staff_water_cleaning().is_some_and(|policy| {
                tank_water_quality
                    .get_mut(job.target)
                    .is_ok_and(|mut water_quality| {
                        let quality_permille =
                            (policy.value.clamp(0.0, 1.0) * 1000.0).round() as u16;
                        water_quality.0 = quality_permille;
                        cleaned_tank_water.write(TankWaterCleaned {
                            staff: claim.staff,
                            tank: job.target,
                            policy: AssetId(policy.token.0),
                            quality_permille,
                            localization_key: AssetId(policy.localization_key.0),
                        });
                        true
                    })
            }),
            _ => false,
        };
        if applied {
            completed_jobs.write(StaffJobCompleted {
                staff: claim.staff,
                job: job_entity,
                kind: job.kind,
                target: job.target,
            });
        }
    }
}
