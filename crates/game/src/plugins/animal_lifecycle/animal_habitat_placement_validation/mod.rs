use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::aquatic::aquatic_simulation_types::AquaticAnimal;
use crate::plugins::aquatic::aquatic_simulation_types::AquaticWaterRequirement;
use crate::plugins::aquatic::aquatic_simulation_types::LandWaterRequirement;
use crate::plugins::aquatic::aquatic_simulation_types::MarinePlacementFailure;
use crate::plugins::aquatic::aquatic_simulation_types::Tank;
use crate::plugins::aquatic::aquatic_simulation_types::TankCapacity;
use crate::plugins::aquatic::aquatic_simulation_types::TankEnvironment;
use crate::plugins::aquatic::aquatic_simulation_types::TankGeometry;
use crate::plugins::aquatic::aquatic_simulation_types::TankWater;
use crate::plugins::aquatic::aquatic_simulation_types::WaterQuality;
use crate::plugins::aquatic::aquatic_suitability_and_placement_calculations::validate_marine_placement;

pub(super) type AnimalAquaticHabitatQueries<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Tank,
        &'static TankGeometry,
        &'static TankCapacity,
        &'static TankWater,
        &'static WaterQuality,
        &'static TankEnvironment,
    ),
>;

pub(super) fn validate_animal_habitat_against_authored_aquatic_requirements(
    world_definitions: WorldDefinitionsView<'_>,
    species: AssetId,
    habitat: Entity,
    aquatic_habitats: &AnimalAquaticHabitatQueries<'_, '_>,
) -> Result<(), MarinePlacementFailure> {
    let Some(requirement) = world_definitions.find_aquatic_requirement(species) else {
        return Ok(());
    };
    let Ok((_tank, geometry, capacity, water, quality, environment)) =
        aquatic_habitats.get(habitat)
    else {
        return Err(MarinePlacementFailure::NotTank);
    };
    validate_marine_placement(
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
        *geometry,
        *capacity,
        *water,
        *quality,
        *environment,
    )
}
