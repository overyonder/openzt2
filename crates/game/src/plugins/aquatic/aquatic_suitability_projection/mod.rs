use bevy::prelude::*;

use crate::plugins::{
    animal_health::types::Dead, animal_lifecycle::types::Animal,
    animal_welfare::types::HabitatSuitability,
};

use super::{
    aquatic_simulation_types::{
        AquaticAnimal, AquaticHome, AquaticSuitability, AquaticWaterRequirement,
        LandWaterRequirement, Tank, TankCapacity, TankEnvironment, TankGeometry, TankWater,
        WaterQuality,
    },
    aquatic_suitability_and_placement_calculations::calculate_aquatic_suitability,
};

#[allow(clippy::type_complexity)]
pub(super) fn project_changed_tank_facts_into_live_aquatic_animal_suitability(
    tanks: Query<
        (
            Ref<TankGeometry>,
            Ref<WaterQuality>,
            Ref<TankCapacity>,
            Ref<TankWater>,
            Ref<TankEnvironment>,
        ),
        With<Tank>,
    >,
    mut aquatic_animals: Query<
        (
            &AquaticAnimal,
            &AquaticWaterRequirement,
            &LandWaterRequirement,
            &AquaticHome,
            &mut AquaticSuitability,
            &mut HabitatSuitability,
        ),
        (With<Animal>, Without<Dead>),
    >,
) {
    for (animal, water_requirement, land_requirement, home, mut aquatic, mut habitat) in
        &mut aquatic_animals
    {
        let Ok((geometry, quality, capacity, water, environment)) = tanks.get(home.0) else {
            continue;
        };
        if !(geometry.is_changed()
            || quality.is_changed()
            || capacity.is_changed()
            || water.is_changed()
            || environment.is_changed())
        {
            continue;
        }
        let next_suitability = calculate_aquatic_suitability(
            *animal,
            *water_requirement,
            *land_requirement,
            *geometry,
            *capacity,
            *water,
            *quality,
            *environment,
        );
        if *aquatic != next_suitability {
            *aquatic = next_suitability;
            habitat.space = next_suitability.depth.min(next_suitability.space);
            habitat.biome = next_suitability.water.min(next_suitability.land_water);
            habitat.overall = next_suitability.overall;
        }
    }
}
