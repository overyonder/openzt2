//! Event-driven habitat regions derived from fence topology and loaded terrain.

mod habitat_fence_inputs;
mod habitat_membership_and_containment;
pub mod habitat_region_derivation;
mod habitat_region_rebuilding;
mod habitat_terrain_summary_calculation;
mod habitat_terrain_summary_refresh;
pub mod placement_sectors;

use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged;
use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::RebuildHabitatRegionsWithinTopologyBounds;
use openzt2_zoo_state::habitat::habitat_types::RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds;
use openzt2_zoo_state::habitat::habitat_types::{
    HabitatCellOf, HabitatIndex, HabitatTopologyPublication,
};

pub struct HabitatPlugin;

impl Plugin for HabitatPlugin {
    fn build(&self, application: &mut App) {
        application
            .register_required_components::<WorldRoot, HabitatIndex>()
            .register_required_components::<WorldRoot, habitat_fence_inputs::HabitatFenceLattice>()
            .add_observer(habitat_fence_inputs::retire_empty_raster_point)
            .add_observer(habitat_fence_inputs::discard_habitat_fence_lattice_point)
            .add_message::<habitat_fence_inputs::HabitatFenceInputDiscarded>()
            .add_observer(habitat_fence_inputs::discard_habitat_fence_node)
            .add_observer(habitat_fence_inputs::discard_habitat_fence_raster)
            .add_message::<HabitatCellOwnershipChanged>()
            .add_message::<habitat_membership_and_containment::LostHabitatMembership>()
            .add_observer(habitat_membership_and_containment::retire_empty_habitat_location_cell::<habitat_membership_and_containment::HabitatCellOccupants>)
            .add_observer(habitat_membership_and_containment::retire_empty_habitat_location_cell::<HabitatCellOf>)
            .add_observer(habitat_membership_and_containment::retire_empty_habitat_location_cell::<habitat_membership_and_containment::HabitatCellSectors>)
            .add_observer(habitat_membership_and_containment::discard_habitat_location_cell)
            .add_observer(habitat_membership_and_containment::release_members_of_retired_habitat)
            .add_message::<HabitatChanged>()
            .add_message::<ContainmentChanged>()
            .add_message::<RebuildHabitatRegionsWithinTopologyBounds>()
            .add_message::<RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds>()
            .add_systems(
                Update,
                (
                    habitat_region_rebuilding::request_habitat_region_rebuild_for_added_topology_nodes,
                    habitat_region_rebuilding::request_habitat_region_rebuilds_after_topology_changes,
                    habitat_fence_inputs::project_changed_habitat_fence_inputs,
                    habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain
                        .after_ignore_deferred(habitat_region_rebuilding::request_habitat_region_rebuild_for_added_topology_nodes)
                        .after_ignore_deferred(habitat_region_rebuilding::request_habitat_region_rebuilds_after_topology_changes)
                        .after(habitat_fence_inputs::project_changed_habitat_fence_inputs),
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::habitat::habitat_region_rebuilding::request_habitat_region_rebuild_for_added_topology_nodes,
                    crate::habitat::habitat_region_rebuilding::request_habitat_region_rebuilds_after_topology_changes,
                    habitat_fence_inputs::project_changed_habitat_fence_inputs,
                    habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain
                        .after_ignore_deferred(habitat_region_rebuilding::request_habitat_region_rebuild_for_added_topology_nodes)
                        .after_ignore_deferred(habitat_region_rebuilding::request_habitat_region_rebuilds_after_topology_changes)
                        .after(habitat_fence_inputs::project_changed_habitat_fence_inputs),
                    habitat_membership_and_containment::resolve_changed_habitat_membership_and_containment
                        .after(habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain),
                )
                    .in_set(FixedGameSet::Cleanup)
                    .in_set(HabitatTopologyPublication),
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
                    habitat_fence_inputs::project_changed_habitat_fence_inputs,
                    habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain
                        .after_ignore_deferred(habitat_region_rebuilding::request_initial_habitat_region_rebuild_from_loaded_topology)
                        .after(habitat_fence_inputs::project_changed_habitat_fence_inputs),
                    habitat_membership_and_containment::resolve_changed_habitat_membership_and_containment
                        .after(habitat_region_rebuilding::rebuild_changed_habitat_regions_from_fence_topology_and_terrain),
                ),
            );
    }
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
