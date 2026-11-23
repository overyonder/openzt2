use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle};

use super::aquatic_simulation_types::{
    AquaticAnimal, AquaticSuitability, AquaticWaterRequirement, LandWaterRequirement,
};

#[allow(clippy::type_complexity)]
pub(super) fn reconcile_authored_aquatic_animal_requirements(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut animals: Query<(
        Entity,
        Ref<Animal>,
        Ref<SpeciesHandle>,
        Option<&mut AquaticAnimal>,
        Option<&mut AquaticWaterRequirement>,
        Option<&mut LandWaterRequirement>,
        Has<AquaticSuitability>,
    )>,
) {
    let catalogue_changed = definitions.is_changed() || assets.is_changed();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (entity, animal, species, current, water, land, has_suitability) in &mut animals {
        if !catalogue_changed && !animal.is_added() && !species.is_changed() {
            continue;
        }
        let Some(requirement) = definitions.find_aquatic_requirement(species.species) else {
            if current.is_some() || water.is_some() || land.is_some() || has_suitability {
                commands.entity(entity).remove::<(
                    AquaticAnimal,
                    AquaticWaterRequirement,
                    LandWaterRequirement,
                    AquaticSuitability,
                )>();
            }
            continue;
        };
        let next = AquaticAnimal {
            minimum_depth: f32::from(requirement.min_depth_cm) / 100.0,
            initial_space: requirement.initial_space_m3 as f32,
            additional_space: requirement.additional_space_m3 as f32,
        };
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        let next = AquaticWaterRequirement {
            salinity_permille: requirement.salinity_permille,
            temperature_c: requirement.temperature_c,
            minimum_quality: requirement.water_quality_min,
        };
        if let Some(mut water) = water {
            water.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        let next = LandWaterRequirement {
            land_fraction_permille: requirement.land_fraction_permille,
        };
        if let Some(mut land) = land {
            land.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        if !has_suitability {
            commands
                .entity(entity)
                .insert(AquaticSuitability::default());
        }
    }
}
