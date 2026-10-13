use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_health::types::AnimalCaptured;
use crate::plugins::animal_health::types::CaptureAnimalRequest;
use crate::plugins::animal_health::types::Disease;
use crate::plugins::animal_health::types::Escaped;
use crate::plugins::animal_health::types::Tranquilized;
use crate::plugins::animal_health::types::Treatment;
use crate::plugins::animal_health::types::TreatmentRequest;
use crate::plugins::animal_lifecycle::types::Animal;

use super::{
    staff_job_types::{JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask},
    staff_lifecycle_messages::StaffJobCompleted,
};

#[derive(Component)]
pub(super) struct AwaitingAnimalHealthEffect {
    observed_active_treatment: bool,
}

pub(in crate::plugins::staff) fn request_animal_health_effects_after_authored_staff_behavior_finishes(
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
        if !matches!(job.kind, StaffJobKind::Treat | StaffJobKind::Capture) {
            continue;
        }
        if job.kind == StaffJobKind::Treat && job.token == Some(AssetId::from_key("t_cureanimal")) {
            continue;
        }
        let Ok((disease, escaped, tranquilized)) = animals.get(job.target) else {
            continue;
        };
        let effect_requested = match job.kind {
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
                        animal: job.target,
                        treatment: AssetId(treatment.0),
                        provider: claim.staff,
                    });
                    true
                }),
            StaffJobKind::Capture if escaped.is_some() && tranquilized.is_some() => {
                let Some(execution_id) = behavior_tasks
                    .get(job_entity)
                    .ok()
                    .and_then(|behavior| behavior.execution_id)
                else {
                    continue;
                };
                capture_requests.write(CaptureAnimalRequest {
                    animal: job.target,
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
                .insert(AwaitingAnimalHealthEffect {
                    observed_active_treatment: false,
                });
        }
    }
}

pub(in crate::plugins::staff) fn complete_staff_jobs_after_animal_health_effects_apply(
    mut captured_animals: MessageReader<AnimalCaptured>,
    mut jobs: Query<(
        Entity,
        &StaffJob,
        &JobClaim,
        &mut AwaitingAnimalHealthEffect,
    )>,
    animals: Query<(Option<&Treatment>, Option<&Tranquilized>), With<Animal>>,
    behavior_tasks: Query<&StaffJobBehaviorTask>,
    mut completed_jobs: MessageWriter<StaffJobCompleted>,
) {
    for captured_animal in captured_animals.read() {
        for (job_entity, job, claim, _) in &mut jobs {
            if job.kind == StaffJobKind::Capture
                && job_entity == captured_animal.job
                && job.target == captured_animal.animal
                && claim.staff == captured_animal.staff
                && behavior_tasks.get(job_entity).is_ok_and(|behavior| {
                    behavior.execution_id == Some(captured_animal.execution_id)
                })
            {
                completed_jobs.write(StaffJobCompleted {
                    staff: claim.staff,
                    job: job_entity,
                    kind: job.kind,
                    target: job.target,
                });
            }
        }
    }
    for (job_entity, job, claim, mut awaiting_effect) in &mut jobs {
        let Ok((treatment, tranquilized)) = animals.get(job.target) else {
            continue;
        };
        let effect_applied = match job.kind {
            StaffJobKind::Treat => {
                if treatment.is_some_and(|treatment| treatment.provider == claim.staff) {
                    awaiting_effect.observed_active_treatment = true;
                    false
                } else {
                    awaiting_effect.observed_active_treatment && treatment.is_none()
                }
            }
            StaffJobKind::Tranquilize => tranquilized.is_some(),
            _ => false,
        };
        if effect_applied {
            completed_jobs.write(StaffJobCompleted {
                staff: claim.staff,
                job: job_entity,
                kind: job.kind,
                target: job.target,
            });
        }
    }
}
