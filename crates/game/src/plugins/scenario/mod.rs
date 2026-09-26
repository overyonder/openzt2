pub mod campaign_progress_types;
mod challenge_lua_ui;
mod challenge_offer_execution;
mod challenge_offer_lua_validation;
mod challenge_offer_presentation;
pub mod challenge_offer_types;
mod scenario_goal_and_challenge_ui_action_routing;
mod scenario_goal_panel_presentation;
mod scenario_instantiation;
mod scenario_lua_command_publication;
mod scenario_lua_game_api;
mod scenario_lua_query_fact_collection;
pub(crate) mod scenario_lua_types;
mod scenario_lua_virtual_machine;
mod scenario_lua_virtual_machine_invalidation;
mod scenario_objective_lua_execution;
mod scenario_objective_progression;
pub mod scenario_objective_types;
mod scenario_resolution;
mod scenario_selection_ui_action_routing;
pub mod scenario_session_types;
mod scenario_ui_types;

use challenge_offer_types::{
    CreateScenarioChallengeOfferFromLuaSourcePathRequest, RespondToScenarioChallengeRequest,
    ScenarioChallengeAcceptanceRejected, ScenarioChallengeAccepted,
};
use scenario_objective_types::ScenarioObjectiveStatusChanged;
use scenario_session_types::{ScenarioTerminalResult, SelectedScenarioDocument};

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ScenarioFixedUpdateStage {
    EvaluateObjectiveAndChallengeScripts,
    QueueTerminalScenarioResults,
    ApplyTerminalScenarioResults,
}

pub struct ScenarioPlugin;

impl Plugin for ScenarioPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<scenario_lua_types::ScenarioLuaVirtualMachineContexts>()
            .add_message::<RespondToScenarioChallengeRequest>()
            .add_message::<challenge_offer_types::ScenarioChallengePanelRequest>()
            .add_message::<ScenarioChallengeAccepted>()
            .add_message::<ScenarioChallengeAcceptanceRejected>()
            .add_message::<CreateScenarioChallengeOfferFromLuaSourcePathRequest>()
            .add_message::<ScenarioTerminalResult>()
            .add_message::<ScenarioObjectiveStatusChanged>()
            .add_systems(
                Update,
                scenario_lua_virtual_machine_invalidation::
                    invalidate_virtual_machines_affected_by_changed_scenario_or_script_assets,
            )
            .configure_sets(
                FixedUpdate,
                (
                    ScenarioFixedUpdateStage::EvaluateObjectiveAndChallengeScripts,
                    ScenarioFixedUpdateStage::QueueTerminalScenarioResults,
                    ScenarioFixedUpdateStage::ApplyTerminalScenarioResults,
                )
                    .chain()
                    // Challenge maps have no selected campaign document;
                    // their validation and accepted objectives still run.
                    .run_if(scenario_or_challenge_gameplay_is_active),
            )
            .add_systems(
                Update,
                (
                    scenario_goal_and_challenge_ui_action_routing::
                        route_scenario_goal_and_challenge_ui_actions,
                    scenario_selection_ui_action_routing::
                        route_scenario_selection_and_start_ui_actions,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame).or_else(in_state(GamePhase::MapSelection))),
            )
            .add_systems(
                Update,
                (
                    scenario_instantiation::
                        instantiate_selected_scenario_and_authored_objectives,
                    scenario_instantiation::
                        initialize_scenario_challenge_random_number_generator,
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    challenge_offer_execution::
                        create_challenge_offers_requested_by_scenario_scripts,
                    challenge_offer_execution::validate_and_apply_requested_challenge_responses,
                    challenge_offer_presentation::apply_authored_challenge_panel_requests,
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    challenge_offer_presentation::project_authored_challenge_panel_text,
                    scenario_goal_panel_presentation::
                        set_authored_scenario_goal_panel_row_counts,
                    scenario_goal_panel_presentation::
                        connect_authored_scenario_goal_rows_to_live_objectives,
                    scenario_goal_panel_presentation::
                        apply_scenario_goal_panel_filters_to_projected_rows,
                )
                    .chain()
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                challenge_offer_lua_validation::
                    evaluate_daily_lua_validation_scripts_until_one_challenge_is_offered
                    .in_set(FixedGameSet::Think)
                    .in_set(ScenarioFixedUpdateStage::EvaluateObjectiveAndChallengeScripts)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                scenario_objective_lua_execution::
                    evaluate_active_scenario_objective_lua_condition_scripts
                    .in_set(FixedGameSet::Think)
                    .in_set(ScenarioFixedUpdateStage::EvaluateObjectiveAndChallengeScripts)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    scenario_objective_progression::
                        activate_scenario_objectives_with_satisfied_prerequisites,
                    scenario_objective_progression::
                        fail_expired_objectives_and_remove_expired_challenge_offers,
                )
                    .chain()
                    .in_set(FixedGameSet::Think)
                    .in_set(ScenarioFixedUpdateStage::EvaluateObjectiveAndChallengeScripts)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    scenario_objective_lua_execution::
                        execute_scenario_objective_success_and_failure_lua_scripts,
                    scenario_resolution::queue_completed_scenarios_for_terminal_resolution,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .in_set(ScenarioFixedUpdateStage::QueueTerminalScenarioResults)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                scenario_resolution::
                    apply_terminal_scenario_results_and_remove_completed_challenges
                    .in_set(FixedGameSet::Cleanup)
                    .in_set(ScenarioFixedUpdateStage::ApplyTerminalScenarioResults)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                remove_selected_scenario_document_resource_when_leaving_game,
            );
    }
}

fn remove_selected_scenario_document_resource_when_leaving_game(
    mut commands: Commands,
    mut runtime: NonSendMut<scenario_lua_types::ScenarioLuaVirtualMachineContexts>,
) {
    runtime.clear_finished_zoo_session();
    commands.remove_resource::<SelectedScenarioDocument>();
}

fn scenario_or_challenge_gameplay_is_active(
    selected: Option<Res<SelectedScenarioDocument>>,
    worlds: Query<&crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity>,
) -> bool {
    selected.is_some()
        || worlds
            .iter()
            .any(|world| world.mode == crate::game_session_types::WorldSessionMode::Challenge)
}
