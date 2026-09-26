//! Fossil recovery and assembly as focused ECS facts and operations.

pub(crate) mod extinct_animal_observable_fact_types;
mod extinct_animal_relationship_cleanup;
mod extinct_animal_world_hydration;
mod fossil_assembly_lifecycle;
pub(crate) mod fossil_collection_and_assembly_types;
mod fossil_collection_rules;
mod fossil_dig_target_geometry;
mod fossil_recovery_lifecycle;
pub(crate) mod fossil_recovery_types;
mod fossil_sonar_scoring;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};

use self::{
    fossil_collection_and_assembly_types::{
        CollectedFossilPieceInventory, CompletedFossilSetAssembly,
        PlaceFossilPieceInAssemblyRequest,
    },
    fossil_recovery_types::{ActivateFossilSiteMarkersRequest, DiscoverFossilAtSiteRequest},
};

pub(super) struct ExtinctAnimalsPlugin;

impl Plugin for ExtinctAnimalsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CollectedFossilPieceInventory>()
            .add_message::<DiscoverFossilAtSiteRequest>()
            .add_message::<PlaceFossilPieceInAssemblyRequest>()
            .add_message::<ActivateFossilSiteMarkersRequest>()
            .add_message::<CompletedFossilSetAssembly>()
            .add_systems(
                Update,
                (
                    extinct_animal_world_hydration::hydrate_extinct_animal_world_state_from_loaded_definitions,
                    fossil_recovery_lifecycle::activate_all_fossil_site_markers_when_requested,
                    fossil_recovery_lifecycle::update_active_fossil_search_sonar_tracking,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    fossil_assembly_lifecycle::initialize_fossil_assembly_from_first_piece_placement_request,
                    fossil_assembly_lifecycle::place_requested_fossil_pieces_into_assembly_slots,
                    fossil_assembly_lifecycle::request_authored_catalogue_unlocks_for_completed_fossil_sets,
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                fossil_recovery_lifecycle::discover_weighted_fossil_piece_at_requested_site
                    .run_if(
                        any_with_component::<crate::plugins::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode>,
                    )
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                extinct_animal_relationship_cleanup::remove_fossil_assembly_and_piece_links_after_assembly_table_removal
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod fossil_sonar_scoring_tests;
