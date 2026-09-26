pub mod adoption_and_content_availability_types;
pub mod award_and_progression_fact_types;
mod award_condition_calculations;
mod award_evaluation_and_granting;
mod award_ui_action_routing;
mod catalogue_definition_index_queries;
pub(crate) mod catalogue_entry_availability;
mod catalogue_unlock_application;
pub(crate) mod catalogue_unlock_set_operations;
mod catalogue_unlock_storage_initialization;
mod fame_calculations;
mod fame_history_recording;
pub mod fame_history_types;
mod fame_progression;
pub mod fame_types;
mod profile_challenge_fact_recording;
pub mod profile_challenge_types;
mod progression_session_reset;
mod progression_text_projection;
pub mod rating_types;
pub(crate) mod research_duration_calculation;
mod research_execution;
mod research_progress_calculations;
pub mod research_types;
mod research_ui_action_routing;
mod research_ui_presentation;
mod scenario_award_point_adjustment;
pub mod unlock_types;
mod zoo_rating_calculations;
mod zoo_rating_recomputation;

use award_and_progression_fact_types::{
    AdjustScenarioAwardPointTotalRequest, ProgressionAwardGrantCandidate, ProgressionAwardGranted,
    ProgressionFactChanged, ScenarioAwardPointTotal,
};
use fame_history_types::FameHistory;
use fame_types::Fame;
use profile_challenge_types::{
    ProfileChallengeCompletionCounts, RecordCompletedChallengeRequest,
    TotalEndangeredAnimalBirthCount,
};
use rating_types::ZooRating;
use research_types::{
    ResearchProjectCompleted, ResearchProjectCompletionReady, StartResearchProjectRequest,
};
use unlock_types::{
    ApplyCatalogueDefinitionUnlockRequest, CatalogueDefinitionUnlockApplied,
    CatalogueDefinitionUnlockRejected, UnlockedCatalogueDefinitionSet,
};

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use crate::plugins::simulation_time::simulation_control_application::simulation_is_running;

pub struct ProgressionPlugin;

impl Plugin for ProgressionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fame>()
            .init_resource::<FameHistory>()
            .init_resource::<ScenarioAwardPointTotal>()
            .init_resource::<TotalEndangeredAnimalBirthCount>()
            .init_resource::<ZooRating>()
            .init_resource::<UnlockedCatalogueDefinitionSet>()
            .init_resource::<ProfileChallengeCompletionCounts>()
            .add_message::<ProgressionFactChanged>()
            .add_message::<StartResearchProjectRequest>()
            .add_message::<ApplyCatalogueDefinitionUnlockRequest>()
            .add_message::<CatalogueDefinitionUnlockApplied>()
            .add_message::<CatalogueDefinitionUnlockRejected>()
            .add_message::<ResearchProjectCompleted>()
            .add_message::<ResearchProjectCompletionReady>()
            .add_message::<ProgressionAwardGranted>()
            .add_message::<RecordCompletedChallengeRequest>()
            .add_message::<ProgressionAwardGrantCandidate>()
            .add_message::<AdjustScenarioAwardPointTotalRequest>()
            .add_systems(
                OnExit(GamePhase::InGame),
                progression_session_reset::reset_all_progression_state_when_game_session_ends,
            )
            .add_systems(
                Update,
                (
                    research_ui_action_routing::
                        route_authored_research_ui_actions_to_start_requests,
                    award_ui_action_routing::
                        route_authored_mark_awards_seen_ui_actions_to_earned_progression_awards,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    catalogue_unlock_storage_initialization::
                        initialize_catalogue_unlock_storage_from_active_world_definitions,
                    award_evaluation_and_granting::initialize_award_condition_duration_storage,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    research_ui_presentation::project_selected_catalogue_research_panels
                        .in_set(crate::plugins::ui::UiSet::DomainProjection),
                    research_ui_presentation::project_selected_catalogue_research_progress
                        .in_set(crate::plugins::ui::UiSet::DomainProjection),
                    progression_text_projection::project_progression_facts_into_authored_ui_text,
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    zoo_rating_recomputation::recompute_animal_welfare_rating,
                    zoo_rating_recomputation::recompute_guest_satisfaction_rating,
                    zoo_rating_recomputation::recompute_education_rating,
                    zoo_rating_recomputation::recompute_variety_rating,
                    zoo_rating_recomputation::recompute_scenery_rating,
                    zoo_rating_recomputation::recompute_finance_rating,
                    zoo_rating_recomputation::recompute_cleanliness_rating,
                    zoo_rating_recomputation::recompute_overall_rating,
                    award_evaluation_and_granting::evaluate_all_progression_award_conditions_and_queue_grants
                        .run_if(simulation_is_running),
                    fame_progression::update_fame_and_apply_authored_fame_unlocks,
                    profile_challenge_fact_recording::
                        record_endangered_animal_births_for_profile_challenges,
                    research_execution::refresh_authored_research_availability,
                    research_execution::
                        validate_start_research_requests_and_request_payments,
                    research_execution::advance_research_availability_unlock_countdowns
                        .run_if(simulation_is_running),
                    research_execution::advance_active_research_projects_by_one_tick
                        .run_if(simulation_is_running),
                    fame_history_recording::record_current_fame_in_monthly_history,
                )
                    .chain()
                    .in_set(FixedGameSet::Economy)
                    .run_if(in_state(GamePhase::InGame))
                    .after(crate::plugins::maintenance::zoo_cleanliness_projection::project_changed_maintenance_totals_into_zoo_cleanliness),
            )
            .add_systems(
                FixedUpdate,
                (
                    research_execution::start_paid_research_projects,
                    research_execution::discard_rejected_research_payment_operations,
                    research_execution::complete_research_projects_and_apply_authored_unlocks,
                    catalogue_unlock_application::apply_catalogue_definition_unlock_requests,
                    award_evaluation_and_granting::grant_earned_progression_awards,
                    catalogue_unlock_application::
                        apply_authored_catalogue_unlock_requirements_from_progression_facts,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    scenario_award_point_adjustment::
                        apply_requested_scenario_award_point_total_adjustments,
                    profile_challenge_fact_recording::
                        record_completed_profile_challenge_family_counts,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}
