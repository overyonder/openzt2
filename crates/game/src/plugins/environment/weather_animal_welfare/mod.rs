use bevy::prelude::*;

use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::animal_welfare::types::AdjustEnvironment;
use crate::plugins::habitat::habitat_types::HabitatMember;

use super::{environment_state_types::WorldEnvironment, weather_types::Weather};

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
