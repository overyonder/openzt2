mod aquatic_animal_requirement_hydration;
mod aquatic_home_invalidation;
pub(crate) mod aquatic_simulation_types;
pub(crate) mod aquatic_suitability_and_placement_calculations;
mod aquatic_suitability_projection;
mod show_tank_eligibility_projection;
mod tank_capacity_accounting;
mod tank_fact_hydration;
mod tank_geometry_projection;
mod tank_normalized_fact_clamping;
mod tank_water_quality_decay_calculation;
mod tank_water_quality_decay_execution;

use bevy::prelude::*;

use crate::application_schedule::FixedGameSet;

use aquatic_animal_requirement_hydration::hydrate_live_animals_from_authored_aquatic_requirements;
use aquatic_home_invalidation::{
    invalidate_aquatic_homes_after_tank_or_habitat_region_removal,
    invalidate_terminal_aquatic_animal_homes,
};
use aquatic_suitability_projection::project_changed_tank_facts_into_live_aquatic_animal_suitability;
use show_tank_eligibility_projection::project_changed_tank_depth_into_show_eligibility;
use tank_capacity_accounting::update_tank_capacity_from_changed_aquatic_animal_containment;
use tank_fact_hydration::{
    hydrate_new_tank_surface_geometry_from_authored_offsets,
    hydrate_tank_capacity_population_and_filtration_facts,
};
use tank_geometry_projection::project_changed_habitat_and_terrain_geometry_into_tanks;
use tank_normalized_fact_clamping::clamp_changed_normalized_tank_facts_to_permille_range;
use tank_water_quality_decay_execution::decay_tank_water_quality_after_advanced_zoo_days;

pub(crate) struct AquaticPlugin;

impl Plugin for AquaticPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                hydrate_live_animals_from_authored_aquatic_requirements,
                hydrate_tank_capacity_population_and_filtration_facts,
                hydrate_new_tank_surface_geometry_from_authored_offsets,
            )
                .chain()
                .in_set(FixedGameSet::Act),
        )
        .add_systems(
            FixedUpdate,
            (
                update_tank_capacity_from_changed_aquatic_animal_containment,
                clamp_changed_normalized_tank_facts_to_permille_range,
                project_changed_tank_facts_into_live_aquatic_animal_suitability,
                project_changed_tank_depth_into_show_eligibility,
                decay_tank_water_quality_after_advanced_zoo_days,
            )
                .chain()
                .in_set(FixedGameSet::Think),
        )
        .add_systems(
            FixedUpdate,
            (
                project_changed_habitat_and_terrain_geometry_into_tanks,
                invalidate_terminal_aquatic_animal_homes,
                invalidate_aquatic_homes_after_tank_or_habitat_region_removal,
            )
                .chain()
                .in_set(FixedGameSet::Cleanup),
        );
    }
}

#[cfg(test)]
mod aquatic_suitability_and_placement_calculation_tests;

#[cfg(test)]
mod tank_water_quality_decay_calculation_tests;
