//! Event-driven habitat regions derived from fence topology and loaded terrain.

pub(crate) mod habitat_membership_and_containment;
mod habitat_region_derivation;
mod habitat_region_rebuilding;
mod habitat_terrain_summary_calculation;
mod habitat_terrain_summary_refresh;
mod habitat_topology_cell_queries;
pub(crate) mod habitat_types;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::FixedGameSet;

use habitat_types::{
    ContainmentChanged, HabitatChanged, HabitatIndex, RebuildHabitatRegionsWithinTopologyBounds,
    RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds,
};

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
                    habitat_region_rebuilding::request_habitat_region_rebuild_for_added_topology_nodes,
                    habitat_region_rebuilding::request_habitat_region_rebuilds_after_topology_changes,
                    habitat_terrain_summary_refresh::request_habitat_summary_refreshes_after_terrain_changes,
                    habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain,
                    habitat_terrain_summary_refresh::refresh_terrain_derived_habitat_summaries,
                    habitat_membership_and_containment::update_habitat_membership_after_locatable_entities_move,
                    habitat_membership_and_containment::update_containment_after_habitat_regions_change,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                OnEnter(GamePhase::InGame),
                habitat_region_rebuilding::request_initial_habitat_region_rebuild_from_loaded_topology,
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
