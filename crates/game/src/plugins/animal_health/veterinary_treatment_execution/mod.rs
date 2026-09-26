use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffRoleKind, AssetId};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::staff::staff_employment_types::Staff;
use crate::plugins::staff::staff_employment_types::StaffRole;

use super::{
    disease_progression_calculations::apply_treatment_delta_to_disease_severity,
    types::{Dead, Disease, Treatment, TreatmentRequest, Vitality},
};

pub(super) fn validate_veterinary_treatment_requests_and_begin_treatment(
    mut commands: Commands,
    mut treatment_requests: MessageReader<TreatmentRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animals: Query<(Option<&Disease>, Option<&Treatment>), (With<Animal>, Without<Dead>)>,
    treatment_providers: Query<&StaffRole, With<Staff>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for request in treatment_requests.read() {
        let Ok((Some(disease), None)) = animals.get(request.animal) else {
            continue;
        };
        let (Some(treatment_definition), Ok(provider_role)) = (
            world_definitions.find_treatment(request.treatment),
            treatment_providers.get(request.provider),
        ) else {
            continue;
        };
        let Some(disease_definition) = world_definitions.find_disease(disease.definition) else {
            continue;
        };
        if treatment_definition.disease != disease.definition
            || disease_definition
                .treatments
                .iter()
                .all(|identifier| identifier.0 != request.treatment.0)
            || !staff_role_can_apply_treatment(
                world_definitions,
                provider_role.0,
                &treatment_definition.required_staff,
            )
        {
            continue;
        }
        commands.entity(request.animal).insert(Treatment {
            definition: request.treatment,
            provider: request.provider,
            remaining_ticks: treatment_definition.duration_ticks,
        });
    }
}

pub(super) fn advance_active_veterinary_treatments_and_apply_completed_results(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut animals: Query<
        (Entity, &mut Treatment, &mut Vitality, Option<&mut Disease>),
        (With<Animal>, Without<Dead>),
    >,
    live_treatment_providers: Query<(), With<Staff>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (animal, mut treatment, mut vitality, disease) in &mut animals {
        if live_treatment_providers.get(treatment.provider).is_err() {
            commands.entity(animal).remove::<Treatment>();
            continue;
        }
        treatment.remaining_ticks = treatment.remaining_ticks.saturating_sub(1);
        if treatment.remaining_ticks != 0 {
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
        commands.entity(animal).remove::<Treatment>();
        if disease_cleared {
            commands.entity(animal).remove::<Disease>();
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
