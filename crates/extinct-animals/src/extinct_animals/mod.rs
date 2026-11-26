//! Fossil recovery and assembly as focused ECS facts and operations.

mod behavior_artifact_execution;
mod extinct_animal_world_hydration;
mod fossil_assembly;
pub use fossil_assembly::{FossilAssemblyPiecePose, UpdateFossilAssemblyPiecePose};
mod fossil_definition_index;
mod fossil_dig_target_geometry;
mod fossil_discovery;
mod fossil_find_effect;
mod fossil_sonar;
mod fossil_sonar_scoring;
mod fossil_world_objects;

use bevy::{ecs::schedule::common_conditions::on_message, prelude::*};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::extinct_animals::{
    fossil_collection_and_assembly_types::{
        CompletedFossilSetAssembly, PlaceFossilPieceInAssemblyRequest,
    },
    fossil_recovery_types::{ActivateFossilSiteMarkersRequest, DiscoverFossilAtSiteRequest},
};

pub struct ExtinctAnimalsPlugin;

impl Plugin for ExtinctAnimalsPlugin {
    fn build(&self, app: &mut App) {
        fossil_sonar::register_fossil_sonar_projection(app);
        fossil_find_effect::register(app);
        behavior_artifact_execution::register(app);
        app.add_observer(fossil_discovery::discover_authored_fossil);
        app.init_resource::<fossil_definition_index::FossilDefinitionIndex>()
            .add_message::<DiscoverFossilAtSiteRequest>()
            .add_message::<PlaceFossilPieceInAssemblyRequest>()
            .add_message::<UpdateFossilAssemblyPiecePose>()
            .add_message::<ActivateFossilSiteMarkersRequest>()
            .add_message::<CompletedFossilSetAssembly>()
            .add_message::<crate::fossil_site_types::SetFossilPuzzleToFind>()
            .add_observer(fossil_assembly::clear_assembly_when_table_marker_is_removed)
            .add_observer(fossil_discovery::present_fossil_find_feedback)
            .add_systems(
                Update,
                (
                    extinct_animal_world_hydration::hydrate_extinct_animal_world_state_from_loaded_definitions,
                    fossil_definition_index::index_winning_fossil_definitions,
                    fossil_definition_index::bind_fossil_piece_pictures
                        .after(fossil_assembly::place_requested_fossil_pieces)
                        .after(fossil_assembly::update_fossil_assembly_piece_poses),
                    fossil_assembly::update_fossil_assembly_piece_poses
                        .after(fossil_definition_index::index_winning_fossil_definitions)
                        .after(extinct_animal_world_hydration::hydrate_extinct_animal_world_state_from_loaded_definitions)
                        .after(fossil_world_objects::reset_requested_fossil_search)
                        .run_if(on_message::<UpdateFossilAssemblyPiecePose>),
                    fossil_world_objects::bind_live_fossil_assembly_tables
                        .before(extinct_animal_world_hydration::hydrate_extinct_animal_world_state_from_loaded_definitions),
                    fossil_world_objects::spawn_world_fossil_recovery_sites
                        .after(fossil_definition_index::index_winning_fossil_definitions)
                        .after(fossil_world_objects::reset_requested_fossil_search),
                    fossil_world_objects::reset_requested_fossil_search
                        .run_if(on_message::<crate::fossil_site_types::SetFossilPuzzleToFind>),
                    fossil_assembly::place_requested_fossil_pieces
                        .after(fossil_assembly::update_fossil_assembly_piece_poses)
                        .after(fossil_definition_index::index_winning_fossil_definitions)
                        .after(fossil_world_objects::reset_requested_fossil_search)
                        .after(extinct_animal_world_hydration::hydrate_extinct_animal_world_state_from_loaded_definitions)
                        .run_if(on_message::<PlaceFossilPieceInAssemblyRequest>),
                    fossil_assembly::unlock_catalogue_after_fossil_assembly
                        .after(fossil_assembly::place_requested_fossil_pieces),
                    fossil_discovery::settle_fossil_cash_feedback
                        .run_if(any_with_component::<fossil_discovery::PendingFossilCashReward>),
                    fossil_sonar::activate_sonar_markers_for_search_session
                        .run_if(on_message::<ActivateFossilSiteMarkersRequest>),
                    fossil_sonar::update_fossil_sonar
                        .after(fossil_sonar::activate_sonar_markers_for_search_session)
                        .run_if(any_with_component::<fossil_sonar::ActiveFossilSonar>),
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                fossil_discovery::discover_requested_fossils
                    .in_set(FixedGameSet::Act)
                    .run_if(on_message::<DiscoverFossilAtSiteRequest>)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod fossil_sonar_scoring_tests;
