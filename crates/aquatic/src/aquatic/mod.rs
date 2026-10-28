mod aquatic_animal_requirement_hydration;
mod aquatic_home_invalidation;
use openzt2_zoo_state::aquatic::aquatic_simulation_types;
use openzt2_zoo_state::aquatic::aquatic_suitability_and_placement_calculations;
mod aquatic_suitability_projection;
mod show_tank_eligibility_projection;
mod tank_capacity_accounting;
mod tank_fact_hydration;
mod tank_geometry_edit_execution;
mod tank_geometry_edit_interaction;
use openzt2_zoo_state::aquatic::tank_geometry_edit_types;
mod tank_geometry_projection;
mod tank_geometry_rules;
mod tank_height_bounds;
mod tank_normalized_fact_clamping;
mod tank_presentation;
mod tank_wall_enclosure_promotion;
mod tank_water_quality_decay_calculation;
mod tank_water_quality_decay_execution;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;
use tank_geometry_edit_types::{
    CommitTankGeometryEdit, TankGeometryEditAcknowledged, TankGeometryEditPreparationRejected,
    TankGeometryEditPrepared,
};

use aquatic_animal_requirement_hydration::hydrate_live_animals_from_authored_aquatic_requirements;
use aquatic_home_invalidation::{
    invalidate_aquatic_homes_after_tank_or_habitat_region_removal,
    invalidate_terminal_aquatic_animal_homes,
};
use aquatic_suitability_projection::project_changed_tank_facts_into_live_aquatic_animal_suitability;
use show_tank_eligibility_projection::project_changed_tank_depth_into_show_eligibility;
use tank_capacity_accounting::update_tank_capacity_from_changed_aquatic_animal_containment;
use tank_fact_hydration::hydrate_tank_capacity_population_and_filtration_facts;
use tank_geometry_projection::project_changed_habitat_and_terrain_geometry_into_tanks;
use tank_normalized_fact_clamping::clamp_changed_normalized_tank_facts_to_permille_range;
use tank_water_quality_decay_execution::decay_tank_water_quality_after_advanced_zoo_days;

pub struct AquaticPlugin;

impl Plugin for AquaticPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommitTankGeometryEdit>()
            .add_message::<TankGeometryEditAcknowledged>()
            .add_message::<TankGeometryEditPrepared>()
            .add_message::<TankGeometryEditPreparationRejected>()
            .add_systems(
                Update,
                tank_geometry_edit_interaction::update_tank_geometry_gesture
                    .after(GameSet::Intent)
                    .before(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                tank_geometry_edit_execution::prepare_tank_geometry_construction_edits
                    .in_set(FixedConstructionSet::Prepare)
                    .after(FixedConstructionTransactionMaterializationSet)
                    .before(FixedConstructionPreparedDomainEditMaterializationSet),
            )
            .add_systems(
                FixedUpdate,
                tank_geometry_edit_execution::apply_tank_geometry_construction_edits
                    .in_set(FixedConstructionSet::Apply)
                    .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet),
            )
            .add_systems(
                FixedUpdate,
                hydrate_live_animals_from_authored_aquatic_requirements.in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    tank_fact_hydration::initialize_legacy_tank_water_limits,
                    hydrate_tank_capacity_population_and_filtration_facts,
                    update_tank_capacity_from_changed_aquatic_animal_containment,
                    clamp_changed_normalized_tank_facts_to_permille_range,
                    decay_tank_water_quality_after_advanced_zoo_days,
                    project_changed_tank_facts_into_live_aquatic_animal_suitability,
                    project_changed_tank_depth_into_show_eligibility,
                )
                    .chain()
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    tank_wall_enclosure_promotion::promote_or_restore_habitats_enclosed_by_tank_walls,
                    project_changed_habitat_and_terrain_geometry_into_tanks,
                    invalidate_terminal_aquatic_animal_homes,
                    invalidate_aquatic_homes_after_tank_or_habitat_region_removal,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                Update,
                (
                    tank_presentation::remove_orphaned_tank_presentations,
                    tank_presentation::present_changed_tank_water_and_glass,
                )
                    .chain()
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod aquatic_suitability_and_placement_calculation_tests;

#[cfg(test)]
mod tank_water_quality_decay_calculation_tests;

#[cfg(test)]
mod tank_geometry_edit_tests;
