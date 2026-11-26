mod aquatic_animal_requirement_hydration;
mod aquatic_home_invalidation;
use openzt2_zoo_state::aquatic::aquatic_simulation_types;
use openzt2_zoo_state::aquatic::aquatic_suitability_and_placement_calculations;
mod aquatic_suitability_projection;
mod show_tank_eligibility_projection;
mod tank_capacity_accounting;
mod tank_fact_hydration;
mod tank_floor_terrain_excavation;
mod tank_geometry_edit_execution;
mod tank_geometry_edit_interaction;
use openzt2_zoo_state::aquatic::tank_geometry_edit_types;
mod tank_geometry_projection;
mod tank_geometry_rules;
mod tank_height_bounds;
mod tank_normalized_fact_clamping;
mod tank_presentation;
mod tank_scripted_wall_height;
mod tank_wall_enclosure_promotion;
mod water_filter_operation;
pub mod water_filter_timing;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::ui::UiSet;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::{GameSet, WaterPresentationSet};
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;
use tank_geometry_edit_types::{
    CommitTankGeometryEdit, TankGeometryEditAcknowledged, TankGeometryEditPreparationRejected,
    TankGeometryEditPrepared,
};

use aquatic_suitability_projection::project_changed_tank_facts_into_live_aquatic_animal_suitability;
use tank_geometry_projection::project_changed_habitat_and_terrain_geometry_into_tanks;
use tank_normalized_fact_clamping::clamp_changed_normalized_tank_facts_to_permille_range;
use water_filter_operation::{
    add_animal_dirt_to_home_tank_water, clean_tank_water_through_filters,
    reconcile_authored_water_filter_policy, resolve_water_filter_targets,
};

pub struct AquaticPlugin;

impl Plugin for AquaticPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<aquatic_simulation_types::AddAquaticWaterDirt>()
            .register_required_components::<aquatic_simulation_types::WaterQuality, water_filter_operation::WaterQualityFraction>()
            .register_required_components_with::<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionSource, aquatic_simulation_types::WaterQuality>(|| aquatic_simulation_types::WaterQuality(1000))
            .register_required_components_with::<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionEdit, aquatic_simulation_types::WaterQuality>(|| aquatic_simulation_types::WaterQuality(1000))
            .add_message::<CommitTankGeometryEdit>()
            .add_message::<TankGeometryEditAcknowledged>()
            .add_message::<TankGeometryEditPrepared>()
            .add_message::<TankGeometryEditPreparationRejected>()
            .add_message::<tank_geometry_edit_types::SetTankWallHeights>()
            .add_message::<tank_floor_terrain_excavation::RestoreTankFloorTerrain>()
            .add_message::<tank_geometry_projection::TankFloorTerrainDisturbed>()
            .add_message::<tank_capacity_accounting::TankResidentRequirementsDiscarded>()
            .add_observer(tank_capacity_accounting::invalidate_discarded_tank_resident_requirements)
            .add_observer(tank_capacity_accounting::invalidate_disabled_tank_resident)
            .add_observer(tank_capacity_accounting::invalidate_reenabled_tank_resident)
            .add_observer(aquatic_home_invalidation::notify_residents_of_tank_demotion)
            .add_observer(tank_floor_terrain_excavation::queue_terrain_restoration_after_tank_removal)
            .add_observer(tank_presentation::retire_tank_surfaces_on_demotion)
            .add_observer(tank_presentation::retire_tank_wall_panel)
            .add_observer(tank_geometry_edit_execution::restore_uncommitted_tank_geometry)
            .add_observer(tank_geometry_edit_execution::restore_unprepared_tank_geometry)
            .add_systems(
                Update,
                tank_geometry_edit_execution::capture_submitted_tank_geometry
                    .in_set(openzt2_construction::construction::construction_transaction_preparation::ConstructionSubmissionSet),
            )
            .add_systems(
                FixedUpdate,
                tank_geometry_edit_execution::clear_submitted_tank_geometry
                    .after(FixedConstructionSet::Prepare)
                    .before(FixedConstructionSet::Apply),
            )
            .add_observer(water_filter_operation::remove_water_filter_timing_and_target)
            .add_systems(
                Update,
                tank_scripted_wall_height::apply_scripted_tank_wall_heights
                    .after(UiSet::DomainProjection)
                    .before(UiSet::Interaction)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                water_filter_operation::publish_water_filter_dirty_attribute
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                tank_geometry_edit_interaction::update_tank_geometry_gesture
                    .after(GameSet::Intent)
                    .before(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    tank_geometry_projection::sample_changed_tank_floor_levels,
                    tank_floor_terrain_excavation::excavate_changed_tank_floors,
                    tank_floor_terrain_excavation::retire_applied_tank_floor_excavations,
                ).chain()
                    .in_set(GameSet::Presentation)
                    .before(WaterPresentationSet::View),
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
                aquatic_animal_requirement_hydration::reconcile_authored_aquatic_animal_requirements
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    tank_fact_hydration::initialize_legacy_tank_water_limits,
                    tank_capacity_accounting::recompute_changed_tank_capacity,
                    clamp_changed_normalized_tank_facts_to_permille_range,
                    reconcile_authored_water_filter_policy,
                    resolve_water_filter_targets.after(reconcile_authored_water_filter_policy),
                    (
                        add_animal_dirt_to_home_tank_water
                            .after(clamp_changed_normalized_tank_facts_to_permille_range),
                        clean_tank_water_through_filters
                            .after(resolve_water_filter_targets)
                            .run_if(on_message::<openzt2_zoo_state::environment::environment_state_types::ZooTickAdvanced>),
                        project_changed_tank_facts_into_live_aquatic_animal_suitability
                            .after(tank_fact_hydration::initialize_legacy_tank_water_limits)
                            .after(tank_capacity_accounting::recompute_changed_tank_capacity),
                    ).chain(),
                )
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    (
                        tank_wall_enclosure_promotion::promote_or_restore_habitats_enclosed_by_tank_walls,
                        project_changed_habitat_and_terrain_geometry_into_tanks,
                    ).chain(),
                    aquatic_home_invalidation::notify_terminal_aquatic_animal_departures
                        .after(tank_wall_enclosure_promotion::promote_or_restore_habitats_enclosed_by_tank_walls),
                    show_tank_eligibility_projection::reconcile_authored_show_tank_eligibility
                        .after(project_changed_habitat_and_terrain_geometry_into_tanks),
                )
                    .in_set(FixedGameSet::Cleanup)
                    .after(openzt2_zoo_state::habitat::habitat_types::HabitatTopologyPublication),
            )
            .add_systems(
                Update,
                (
                    tank_presentation::project_tank_surface_meshes
                        .before(WaterPresentationSet::View),
                    tank_presentation::project_tank_wall_panels
                        .after(WaterPresentationSet::View),
                    tank_presentation::project_authored_tank_material_passes
                        .after(tank_presentation::project_tank_wall_panels)
                        .after(tank_presentation::project_tank_surface_meshes),
                    tank_presentation::update_tank_wall_water_bindings
                        .after(tank_presentation::project_authored_tank_material_passes)
                        .after(WaterPresentationSet::Environment),
                )
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod aquatic_suitability_and_placement_calculation_tests;

#[cfg(test)]
mod tank_geometry_edit_tests;

#[cfg(test)]
mod tank_floor_terrain_excavation_tests;
