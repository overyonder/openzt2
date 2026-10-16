//! Event-driven habitat regions derived from fence topology and loaded terrain.

mod habitat_membership_and_containment;
mod habitat_region_derivation;
mod habitat_region_rebuilding;
mod habitat_terrain_summary_calculation;
mod habitat_terrain_summary_refresh;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::RebuildHabitatRegionsWithinTopologyBounds;
use openzt2_zoo_state::habitat::habitat_types::RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds;

pub struct HabitatPlugin;

impl Plugin for HabitatPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_resource::<HabitatIndex>()
            .add_message::<HabitatChanged>()
            .add_message::<ContainmentChanged>()
            .add_message::<RebuildHabitatRegionsWithinTopologyBounds>()
            .add_message::<RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds>()
            .add_systems(
                FixedUpdate,
                (
                    crate::habitat::habitat_region_rebuilding::request_habitat_region_rebuild_for_added_topology_nodes,
                    crate::habitat::habitat_region_rebuilding::request_habitat_region_rebuilds_after_topology_changes,
                    crate::habitat::habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain,
                    crate::habitat::habitat_membership_and_containment::update_habitat_membership_after_locatable_entities_move,
                    crate::habitat::habitat_membership_and_containment::update_containment_after_habitat_regions_change,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                Update,
                (
                    crate::habitat::habitat_terrain_summary_refresh::request_habitat_summary_refreshes_after_terrain_changes,
                    crate::habitat::habitat_terrain_summary_refresh::refresh_terrain_derived_habitat_summaries,
                )
                    .chain()
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                OnEnter(GamePhase::InGame),
                (
                    crate::habitat::habitat_region_rebuilding::request_initial_habitat_region_rebuild_from_loaded_topology,
                    crate::habitat::habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain,
                    crate::habitat::habitat_membership_and_containment::update_habitat_membership_after_locatable_entities_move,
                    crate::habitat::habitat_membership_and_containment::update_containment_after_habitat_regions_change,
                ).chain(),
            )
            .add_systems(OnExit(GamePhase::InGame), clear_habitat_index_after_leaving_game);
    }
}

fn clear_habitat_index_after_leaving_game(mut habitat_index: ResMut<HabitatIndex>) {
    *habitat_index = default();
}

#[cfg(test)]
mod habitat_membership_query_tests;
#[cfg(test)]
mod habitat_region_derivation_tests;
#[cfg(test)]
mod habitat_region_rebuilding_tests;
#[cfg(test)]
mod habitat_terrain_summary_calculation_tests;
#[cfg(test)]
mod habitat_terrain_summary_refresh_tests;
#[cfg(test)]
mod habitat_topology_cell_query_tests;
