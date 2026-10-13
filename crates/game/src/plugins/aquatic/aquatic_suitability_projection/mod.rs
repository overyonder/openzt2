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
            Option<Ref<AquaticHome>>,
            &mut AquaticSuitability,
            &mut HabitatSuitability,
        ),
        (With<Animal>, Without<Dead>),
    >,
) {
    for (animal, water_requirement, land_requirement, home, mut aquatic, mut habitat) in
        &mut aquatic_animals
    {
        let next_suitability =
            if let Some((home, (geometry, quality, capacity, water, environment))) = home
                .as_ref()
                .and_then(|home| tanks.get(home.0).ok().map(|facts| (home, facts)))
            {
                if !(home.is_changed()
                    || aquatic.is_added()
                    || habitat.is_added()
                    || geometry.is_changed()
                    || quality.is_changed()
                    || capacity.is_changed()
                    || water.is_changed()
                    || environment.is_changed())
                {
                    continue;
                }
                calculate_aquatic_suitability(
                    *animal,
                    *water_requirement,
                    *land_requirement,
                    *geometry,
                    *capacity,
                    *water,
                    *quality,
                    *environment,
                )
            } else {
                // A removed home or incomplete tank cannot retain its previous suitability.
                AquaticSuitability::default()
            };
        aquatic.set_if_neq(next_suitability);
        habitat.set_if_neq(HabitatSuitability {
            space: next_suitability.depth.min(next_suitability.space),
            biome: next_suitability.water.min(next_suitability.land_water),
            overall: next_suitability.overall,
        });
    }
}
