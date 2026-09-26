use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::Animal;

use super::{
    disease_progression_calculations::advance_disease_severity_hint_and_vitality_by_one_tick,
    types::{Dead, Disease, Vitality},
};

pub(super) fn advance_active_animal_diseases_and_apply_vitality_loss(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut animals: Query<(&mut Disease, &mut Vitality), (With<Animal>, Without<Dead>)>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (mut disease, mut vitality) in &mut animals {
        let Some(disease_definition) = world_definitions.find_disease(disease.definition) else {
            continue;
        };
        let advanced_disease = advance_disease_severity_hint_and_vitality_by_one_tick(
            disease.elapsed_ticks,
            disease.severity_permille,
            disease_definition.severity_per_tick_q16,
            disease_definition.vitality_per_tick_q16,
            disease_definition.fatal_threshold,
            disease_definition.hint_thresholds,
        );
        disease.elapsed_ticks = advanced_disease.elapsed_ticks;
        disease.severity_permille = advanced_disease.severity_permille;
        disease.hint_level = advanced_disease.hint_level;
        vitality.adjust_permille(advanced_disease.vitality_delta_permille);
        if advanced_disease.fatal {
            vitality.0 = 0.0;
        }
    }
}
