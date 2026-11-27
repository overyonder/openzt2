use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_health::types::AnimalCaptured;
use openzt2_zoo_state::animal_health::types::CaptureAnimalRequest;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Escaped;
use openzt2_zoo_state::animal_health::types::Tranquilized;
use openzt2_zoo_state::animal_health::types::{
    TreatmentCorrelation, AnimalTreatmentSettled, TreatmentOutcome, TreatmentRequest,
};
use openzt2_zoo_state::animal_lifecycle::types::Animal;

use super::{
    staff_job_types::{JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask},
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
};

#[derive(Component)]
pub(super) struct AwaitingAnimalHealthEffect;

pub(crate) fn request_animal_health_effects_after_authored_staff_behavior_finishes(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut jobs: Query<
        (Entity, &StaffJob, &JobProgress, &JobClaim),
        Without<AwaitingAnimalHealthEffect>,
    >,
    animals: Query<(Option<&Disease>, Option<&Escaped>, Option<&Tranquilized>), With<Animal>>,
    behavior_tasks: Query<&StaffJobBehaviorTask>,
    mut treatment_requests: MessageWriter<TreatmentRequest>,
    mut capture_requests: MessageWriter<CaptureAnimalRequest>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (job_entity, job, _, claim) in &mut jobs {
        let Some(kind @ (StaffJobKind::Treat | StaffJobKind::Capture)) = job.kind else {
            continue;
        };
        if (kind == StaffJobKind::Treat
            && (job.token == Some(AssetId::from_key("t_cureanimal"))
                || job.token == Some(AssetId::from_key("t_healanimal"))))
            || (kind == StaffJobKind::Capture
                && job.token == Some(AssetId::from_key("t_crateanimal")))
        {
            continue;
        }
        let Some(target) = job.target else { continue };
        let Ok((disease, escaped, tranquilized)) = animals.get(target) else {
            continue;
        };
        let Some(execution_id) = behavior_tasks
            .get(job_entity)
            .ok()
            .and_then(|behavior| behavior.execution_id)
        else {
            continue;
        };
        let effect_requested = match kind {
            StaffJobKind::Treat => disease
                .and_then(|disease| catalogue.find_disease(disease.definition))
                .and_then(|disease_definition| {
                    disease_definition
                        .treatments
                        .iter()
                        .find(|treatment| catalogue.find_treatment(**treatment).is_some())
                        .copied()
                })
                .is_some_and(|treatment| {
                    treatment_requests.write(TreatmentRequest {
                        animal: target,
                        treatment: AssetId(treatment.0),
                        provider: claim.staff,
                        correlation: Some(TreatmentCorrelation {
                            job: job_entity,
                            execution_id,
                        }),
                    });
                    true
                }),
            StaffJobKind::Capture if escaped.is_some() && tranquilized.is_some() => {
                capture_requests.write(CaptureAnimalRequest {
                    animal: target,
                    staff: claim.staff,
                    job: job_entity,
                    execution_id,
                });
                true
            }
            _ => false,
        };
        if effect_requested {
            commands
                .entity(job_entity)
                .insert(AwaitingAnimalHealthEffect);
        }
    }
}

pub(crate) fn complete_staff_jobs_after_animal_health_effects_apply(
    mut captured_animals: MessageReader<AnimalCaptured>,
    mut treatments: MessageReader<AnimalTreatmentSettled>,
    jobs: Query<(Entity, &StaffJob, &JobClaim), With<AwaitingAnimalHealthEffect>>,
    behavior_tasks: Query<&StaffJobBehaviorTask>,
    mut completed_jobs: MessageWriter<StaffJobCompleted>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for captured_animal in captured_animals.read() {
        if let Ok((job_entity, job, claim)) = jobs.get(captured_animal.job) {
            if job.kind == Some(StaffJobKind::Capture)
                && job.target == Some(captured_animal.animal)
                && claim.staff == captured_animal.staff
                && behavior_tasks.get(job_entity).is_ok_and(|behavior| {
                    behavior.execution_id == Some(captured_animal.execution_id)
                })
            {
                completed_jobs.write(StaffJobCompleted {
                    staff: claim.staff,
                    job: job_entity,
                    kind: StaffJobKind::Capture,
                    target: captured_animal.animal,
                });
            }
        }
    }
    for treatment in treatments.read() {
        let Ok((job_entity, job, claim)) = jobs.get(treatment.correlation.job) else {
            continue;
        };
        if job.kind != Some(StaffJobKind::Treat)
            || job.target != Some(treatment.animal)
            || claim.staff != treatment.provider
            || !behavior_tasks.get(job_entity).is_ok_and(|behavior| {
                behavior.execution_id == Some(treatment.correlation.execution_id)
            })
        {
            continue;
        }
        match treatment.outcome {
            TreatmentOutcome::Completed => {
                completed_jobs.write(StaffJobCompleted {
                    staff: claim.staff,
                    job: job_entity,
                    kind: StaffJobKind::Treat,
                    target: treatment.animal,
                });
            }
            TreatmentOutcome::Rejected | TreatmentOutcome::Cancelled => {
                cancellations.write(CancelStaffJobRequest { job: job_entity });
            }
        }
    }
}
