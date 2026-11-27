use bevy::ecs::entity::EntityHashSet;
use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_game_data::{world_definitions::staff_management::StaffRoleKind, AssetId};
use openzt2_progression::progression::{AwardHistoryOutcome, RecordAwardOutcome};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::staff::staff_job_types::{
    CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

use super::{
    disease_progression_calculations::apply_treatment_delta_to_disease_severity,
    types::{
        AnimalTreatmentSettled, Dead, Disease, Treatment, TreatmentClock, TreatmentCorrelation,
        TreatmentOutcome, TreatmentRequest, Vitality,
    },
};

pub(super) fn cancel_removed_treatment(
    removed: On<Remove<Treatment>>,
    treatments: Query<(&Treatment, &TreatmentClock)>,
    mut results: MessageWriter<AnimalTreatmentSettled>,
    mut commands: Commands,
) {
    if let Ok((treatment, _)) = treatments.get(removed.entity) {
        if let Some(correlation) = treatment.correlation {
            results.write(AnimalTreatmentSettled {
                animal: removed.entity,
                provider: treatment.provider,
                correlation,
                outcome: TreatmentOutcome::Cancelled,
            });
        }
        commands
            .entity(removed.entity)
            .try_remove::<TreatmentClock>();
    }
}

pub(super) fn validate_veterinary_treatment_requests_and_begin_treatment(
    mut commands: Commands,
    mut treatment_requests: MessageReader<TreatmentRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animals: Query<(Option<&Disease>, Option<&Treatment>), (With<Animal>, Without<Dead>)>,
    treatment_providers: Query<(&StaffRole, Option<&CurrentJob>), With<Staff>>,
    jobs: Query<(&StaffJob, &JobClaim, &StaffJobBehaviorTask)>,
    mut results: MessageWriter<AnimalTreatmentSettled>,
    time: Res<Time<Fixed>>,
) {
    let definitions = active_world_definitions.get(&world_definition_assets);
    let mut admitted = EntityHashSet::default();
    let mut seen = HashSet::<(Entity, Entity, u64)>::default();
    for request in treatment_requests.read() {
        if let Some(correlation) = request.correlation {
            let duplicate =
                !seen.insert((request.animal, correlation.job, correlation.execution_id));
            let running = animals.get(request.animal).is_ok_and(|(_, treatment)| {
                treatment.is_some_and(|treatment| treatment.correlation == Some(correlation))
            });
            if duplicate || running {
                continue;
            }
        }
        let provider = treatment_providers.get(request.provider).ok();
        let current_correlation = provider.and_then(|(_, job)| {
            let job = job?.job();
            let (request_job, claim, behavior) = jobs.get(job).ok()?;
            if request_job.kind != Some(StaffJobKind::Treat)
                || request_job.target != Some(request.animal)
                || claim.staff != request.provider
            {
                return None;
            }
            Some(TreatmentCorrelation {
                job,
                execution_id: behavior.execution_id?,
            })
        });
        let duration = (|| {
            if request.correlation.is_some() && request.correlation != current_correlation {
                return None;
            }
            let definitions = definitions?;
            let (Some(disease), None) = animals.get(request.animal).ok()? else {
                return None;
            };
            let treatment = definitions.find_treatment(request.treatment)?;
            let disease_definition = definitions.find_disease(disease.definition)?;
            (treatment.disease == disease.definition
                && disease_definition
                    .treatments
                    .iter()
                    .any(|identifier| identifier.0 == request.treatment.0)
                && staff_role_can_apply_treatment(
                    definitions,
                    provider?.0 .0,
                    &treatment.required_staff,
                ))
            .then_some(treatment.duration_ticks)
        })();
        if let Some(duration) = duration.filter(|_| admitted.insert(request.animal)) {
            commands.entity(request.animal).insert((
                Treatment {
                    definition: request.treatment,
                    provider: request.provider,
                    correlation: request.correlation,
                },
                TreatmentClock {
                    timer: Timer::new(time.timestep().saturating_mul(duration), TimerMode::Once),
                },
            ));
        } else if let Some(correlation) = request.correlation {
            results.write(AnimalTreatmentSettled {
                animal: request.animal,
                provider: request.provider,
                correlation,
                outcome: TreatmentOutcome::Rejected,
            });
        }
    }
}

pub(super) fn advance_active_veterinary_treatments_and_apply_completed_results(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut animals: Query<
        (
            Entity,
            &Treatment,
            &mut TreatmentClock,
            &mut Vitality,
            Option<&mut Disease>,
            Has<Dead>,
            Option<&WorldMember>,
        ),
        With<Animal>,
    >,
    live_treatment_providers: Query<Option<&CurrentJob>, With<Staff>>,
    jobs: Query<(&JobClaim, &StaffJobBehaviorTask)>,
    time: Res<Time<Fixed>>,
    mut cancelled: MessageReader<
        openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest,
    >,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let cancelled = cancelled
        .read()
        .map(|request| request.job)
        .collect::<EntityHashSet>();
    for (animal, treatment, mut clock, mut vitality, disease, dead, member) in &mut animals {
        let provider = live_treatment_providers.get(treatment.provider);
        let invalid_claim = treatment.correlation.is_some_and(|correlation| {
            cancelled.contains(&correlation.job)
                || provider
                    .ok()
                    .flatten()
                    .is_none_or(|current| current.job() != correlation.job)
                || !jobs.get(correlation.job).is_ok_and(|(claim, task)| {
                    claim.staff == treatment.provider
                        && task.execution_id == Some(correlation.execution_id)
                })
        });
        if dead || provider.is_err() || invalid_claim {
            commands.entity(animal).remove::<Treatment>();
            continue;
        }
        let Some(world_definitions) = world_definitions else {
            continue;
        };
        if !clock.timer.tick(time.delta()).is_finished() {
            continue;
        }
        let Some(treatment_definition) = world_definitions.find_treatment(treatment.definition)
        else {
            commands.entity(animal).remove::<Treatment>();
            continue;
        };
        vitality.adjust_permille(i32::from(treatment_definition.vitality_delta));
        let mut disease_cleared = false;
        if let Some(mut disease) = disease {
            if disease.definition == treatment_definition.disease {
                disease.severity_permille = apply_treatment_delta_to_disease_severity(
                    disease.severity_permille,
                    treatment_definition.severity_delta,
                );
                if disease.severity_permille == 0 {
                    disease_cleared = true;
                }
            }
        }
        let correlation = treatment.correlation;
        commands
            .entity(animal)
            .remove::<TreatmentClock>()
            .remove::<Treatment>();
        if disease_cleared {
            commands.entity(animal).remove::<Disease>();
            if let Some(member) = member {
                commands.write_message(RecordAwardOutcome {
                    root: member.root,
                    outcome: AwardHistoryOutcome::DiseaseCured,
                });
            }
        }
        if let Some(correlation) = correlation {
            commands.write_message(AnimalTreatmentSettled {
                animal,
                provider: treatment.provider,
                correlation,
                outcome: TreatmentOutcome::Completed,
            });
        }
    }
}

fn staff_role_can_apply_treatment(
    world_definitions: WorldDefinitionsView<'_>,
    staff_role: AssetId,
    required_staff_kind: &StaffRoleKind,
) -> bool {
    world_definitions
        .find_staff(staff_role)
        .is_some_and(|staff_definition| {
            std::mem::discriminant(&staff_definition.role)
                == std::mem::discriminant(required_staff_kind)
        })
}
