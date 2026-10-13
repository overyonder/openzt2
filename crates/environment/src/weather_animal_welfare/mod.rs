use bevy::prelude::*;
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_welfare::types::AdjustEnvironment;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;

use super::weather_types::Weather;

pub(super) fn apply_current_weather_welfare_adjustment_to_zoo_animals(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    species: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    environments: Query<&Weather, (With<WorldEnvironment>, Changed<Weather>)>,
    animals: Query<(Entity, &SpeciesHandle, &HabitatMember), With<Animal>>,
    mut adjustments: MessageWriter<AdjustEnvironment>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(species_index) = species_index.get(&species) else {
        return;
    };
    for weather in &environments {
        let Some(definition) = definitions.find_weather(weather.definition) else {
            continue;
        };
        let welfare_adjustment_q16 = i32::from(definition.welfare_delta);
        if welfare_adjustment_q16 == 0 {
            continue;
        }
        for (animal, species_handle, _) in &animals {
            if species_index.find(species_handle.species).is_some() {
                adjustments.write(AdjustEnvironment {
                    animal,
                    delta_q16: welfare_adjustment_q16,
                });
            }
        }
    }
}
