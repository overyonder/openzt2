use openzt2_zoo_state::progression::adoption_and_content_availability_types;
use openzt2_zoo_state::progression::award_and_progression_fact_types;
use openzt2_zoo_state::progression::catalogue_definition_index_queries;
use openzt2_zoo_state::progression::catalogue_entry_availability;
use openzt2_zoo_state::progression::catalogue_unlock_set_operations;
use openzt2_zoo_state::progression::fame_history_types;
use openzt2_zoo_state::progression::fame_types;
use openzt2_zoo_state::progression::profile_challenge_types;
use openzt2_zoo_state::progression::rating_types;
use openzt2_zoo_state::progression::research_duration_calculation;
use openzt2_zoo_state::progression::research_types;
use openzt2_zoo_state::progression::unlock_types;
mod authored_fame_script;
mod award_condition_calculations;
mod award_evaluation_and_granting;
mod award_ui_action_routing;
mod catalogue_unlock_application;
mod catalogue_unlock_storage_initialization;
mod fame_calculations;
mod fame_history_recording;
mod fame_progression;
mod profile_challenge_fact_recording;
mod progression_text_projection;
mod research_execution;
mod research_progress_calculations;
mod research_ui_action_routing;
mod research_ui_presentation;
mod scenario_award_point_adjustment;
pub(crate) mod scenario_content_availability;
mod zoo_rating_calculations;
mod zoo_rating_input_types;
mod zoo_rating_recomputation;

use award_and_progression_fact_types::{
    AdjustScenarioAwardPointTotalRequest, ProgressionAwardGrantCandidate, ProgressionAwardGranted,
    ProgressionFactChanged, ScenarioAwardPointTotal,
};
use fame_history_types::FameHistory;
use fame_types::{Fame, ZooMaximumFame};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use profile_challenge_types::{
    ProfileChallengeCompletionCounts, RecordCompletedChallengeRequest, TotalAnimalsReleasedCount,
    TotalEndangeredAnimalBirthCount,
};
use rating_types::ZooRating;
use research_types::{ResearchProjectCompleted, StartResearchProjectRequest};
use unlock_types::{
    ApplyCatalogueDefinitionUnlockRequest, CatalogueDefinitionUnlockApplied,
    CatalogueDefinitionUnlockRejected, UnlockedCatalogueDefinitionSet,
};
use zoo_rating_input_types::{
    AnimalWelfareRating, EarnedAwardFamePoints, GuestRatingFacts, LivingAnimalPopulation,
    ZooFinanceRating, ZooOverallRatingTarget, ZooSceneryRating,
};

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running;
use openzt2_zoo_state::construction::FixedConstructionSet;

pub struct ProgressionPlugin;

impl Plugin for ProgressionPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<WorldRoot, FameHistory>()
            .register_required_components::<WorldRoot, ScenarioAwardPointTotal>()
            .register_required_components::<WorldRoot, TotalEndangeredAnimalBirthCount>()
            .register_required_components::<WorldRoot, TotalAnimalsReleasedCount>()
            .register_required_components::<WorldRoot, Fame>()
            .register_required_components::<WorldRoot, ZooMaximumFame>()
            .register_required_components::<WorldRoot, AnimalWelfareRating>()
            .register_required_components::<WorldRoot, LivingAnimalPopulation>()
            .register_required_components::<WorldRoot, GuestRatingFacts>()
            .register_required_components::<WorldRoot, EarnedAwardFamePoints>()
            .register_required_components::<WorldRoot, ZooSceneryRating>()
            .register_required_components::<WorldRoot, ZooFinanceRating>()
            .register_required_components::<WorldRoot, ZooOverallRatingTarget>()
            .register_required_components::<WorldRoot, ZooRating>()
            .register_required_components::<WorldRoot, UnlockedCatalogueDefinitionSet>()
            .init_resource::<ProfileChallengeCompletionCounts>()
            .add_message::<ProgressionFactChanged>()
            .add_message::<StartResearchProjectRequest>()
            .add_message::<openzt2_zoo_state::progression::research_types::SetResearchDuration>()
            .add_systems(
                Update,
                research_execution::apply_research_duration_overrides.after(
                    catalogue_unlock_storage_initialization::reconcile_authored_research_items,
                ),
            )
            .add_message::<ApplyCatalogueDefinitionUnlockRequest>()
            .add_message::<CatalogueDefinitionUnlockApplied>()
            .add_message::<CatalogueDefinitionUnlockRejected>()
            .add_message::<ResearchProjectCompleted>()
            .add_message::<ProgressionAwardGranted>()
            .add_message::<RecordCompletedChallengeRequest>()
            .add_message::<ProgressionAwardGrantCandidate>()
            .add_message::<AdjustScenarioAwardPointTotalRequest>()
            .add_message::<fame_types::SetScenarioMaximumFame>()
            .add_message::<fame_types::FameGateAppearanceRequested>()
            .add_systems(
                Update,
                authored_fame_script::resolve_winning_fame_script
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_message::<adoption_and_content_availability_types::MakeScenarioContentAvailableRequest>()
            .add_systems(
                Update,
                scenario_content_availability::apply_scenario_content_availability_requests
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
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
                    catalogue_unlock_storage_initialization::reconcile_authored_research_items
                        .after(research_ui_action_routing::route_authored_research_ui_actions_to_start_requests),
                    award_evaluation_and_granting::initialize_award_condition_duration_storage,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    research_ui_presentation::project_selected_catalogue_research_panels
                        .in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
                    research_ui_presentation::project_selected_catalogue_research_progress
                        .in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
                    progression_text_projection::project_progression_facts_into_authored_ui_text,
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                fame_history_recording::record_completed_month_fame_history
                    .after(FixedGameSet::Clock)
                    .before(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    (
                        (
                            zoo_rating_recomputation::reduce_living_animal_population,
                            zoo_rating_recomputation::recompute_animal_welfare_rating,
                            zoo_rating_recomputation::recompute_guest_ratings,
                            zoo_rating_recomputation::recompute_scenery_rating,
                            zoo_rating_recomputation::recompute_finance_rating,
                        ),
                        zoo_rating_recomputation::reconcile_zoo_rating_and_weighted_target
                            .after(openzt2_zoo_state::maintenance::ZooCleanlinessProjection),
                        zoo_rating_recomputation::advance_zoo_rating_smoothing,
                        zoo_rating_recomputation::publish_zoo_rating_changes,
                    ).chain(),
                    profile_challenge_fact_recording::record_released_animals_for_zoo_fame,
                )
                    .in_set(FixedGameSet::Economy)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    profile_challenge_fact_recording::record_endangered_animal_births_for_profile_challenges,
                    research_execution::advance_research_availability_unlock_countdowns
                        .run_if(simulation_is_running),
                    research_execution::advance_active_research_projects_by_one_tick
                        .run_if(simulation_is_running),
                )
                    .in_set(FixedGameSet::Economy)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    (
                        award_evaluation_and_granting::evaluate_all_progression_award_conditions_and_queue_grants,
                        award_evaluation_and_granting::grant_earned_progression_awards,
                        zoo_rating_recomputation::count_earned_award_fame_points,
                        fame_progression::update_fame_and_apply_authored_fame_unlocks
                            .after(scenario_award_point_adjustment::apply_requested_scenario_award_point_total_adjustments),
                        fame_progression::apply_scenario_maximum_fame_requests,
                        fame_progression::publish_world_maximum_fame,
                    ).chain(),
                    research_execution::refresh_authored_research_availability
                        .after_ignore_deferred(fame_progression::publish_world_maximum_fame),
                    research_execution::complete_research_projects_and_apply_authored_unlocks
                        .after_ignore_deferred(fame_progression::update_fame_and_apply_authored_fame_unlocks),
                    catalogue_unlock_application::apply_catalogue_definition_unlock_requests
                        .after(research_execution::complete_research_projects_and_apply_authored_unlocks),
                    catalogue_unlock_application::
                        apply_authored_catalogue_unlock_requirements_from_progression_facts
                        .after(catalogue_unlock_application::apply_catalogue_definition_unlock_requests),
                )
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
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
        add_research_purchase_systems(app);
    }
}

/// Starting research is a player purchase. Like construction, it is paid while
/// the simulation is paused; only its progress waits for the running clock.
fn add_research_purchase_systems(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            research_execution::validate_start_research_requests_and_request_payments
                .in_set(FixedConstructionSet::RequestEconomy),
            (
                research_execution::start_paid_research_projects,
                research_execution::discard_rejected_research_payment_operations,
            )
                .in_set(FixedConstructionSet::CompleteEconomy),
        ),
    );
}
