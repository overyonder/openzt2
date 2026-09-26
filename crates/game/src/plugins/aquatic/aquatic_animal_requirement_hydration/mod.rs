use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;

use super::aquatic_simulation_types::{
    AquaticAnimal, AquaticSuitability, AquaticWaterRequirement, LandWaterRequirement,
};

pub(super) fn hydrate_live_animals_from_authored_aquatic_requirements(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    animals: Query<(Entity, &SpeciesHandle), (With<Animal>, Without<AquaticAnimal>)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, species) in &animals {
        let Some(requirement) = definitions.find_aquatic_requirement(species.species) else {
            continue;
        };
        commands.entity(entity).insert((
            AquaticAnimal {
                minimum_depth: f32::from(requirement.min_depth_cm) / 100.0,
                initial_space: requirement.initial_space_m3 as f32,
                additional_space: requirement.additional_space_m3 as f32,
            },
            AquaticWaterRequirement {
                salinity_permille: requirement.salinity_permille,
                temperature_c: requirement.temperature_c,
                minimum_quality: requirement.water_quality_min,
            },
            LandWaterRequirement {
                land_fraction_permille: requirement.land_fraction_permille,
            },
            AquaticSuitability::default(),
        ));
    }
}
