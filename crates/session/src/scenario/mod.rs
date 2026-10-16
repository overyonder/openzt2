mod challenge_lua_ui;
mod challenge_offer_execution;
mod challenge_offer_lua_validation;
mod challenge_offer_presentation;
pub mod challenge_offer_types;
mod profile_scenario_variable_application;
mod scenario_goal_and_challenge_ui_action_routing;
mod scenario_goal_panel_presentation;
mod scenario_instantiation;
mod scenario_lua_command_publication;
mod scenario_lua_entity_commands;
mod scenario_lua_entity_facts;
mod scenario_lua_entity_queries;
mod scenario_lua_game_api;
mod scenario_lua_photo_challenge_queries;
mod scenario_lua_query_fact_collection;
pub(crate) mod scenario_lua_snapshot;
pub(crate) mod scenario_lua_snapshot_types;
pub(crate) mod scenario_lua_types;
pub(crate) mod scenario_lua_ui;
mod scenario_lua_virtual_machine;
mod scenario_lua_virtual_machine_invalidation;
mod scenario_objective_lua_execution;
mod scenario_objective_progression;
mod scenario_photo_challenge_selection;
mod scenario_resolution;
mod scenario_rule_visibility;
mod scenario_selection_ui_action_routing;
mod scenario_ui_modification_presentation;
mod scenario_ui_types;

use challenge_offer_types::{
    CreateScenarioChallengeOfferFromLuaSourcePathRequest, RespondToScenarioChallengeRequest,
    ScenarioChallengeAcceptanceRejected, ScenarioChallengeAccepted,
};
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatusChanged;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioTerminalResult;
use openzt2_zoo_state::scenario::scenario_session_types::SelectedScenarioDocument;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ScenarioFixedUpdateStage {
    EvaluateObjectiveAndChallengeScripts,
    QueueTerminalScenarioResults,
    ApplyTerminalScenarioResults,
}

pub struct ScenarioPlugin;

impl Plugin for ScenarioPlugin {
    fn build(&self, app: &mut App) {
        scenario_lua_ui::register(app);
        scenario_lua_entity_commands::register(app);
        app.init_non_send::<scenario_lua_types::ScenarioLuaVirtualMachineContexts>()
            .init_resource::<openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables>()
            .add_message::<openzt2_zoo_state::progression::profile_scenario_variables::SetProfileScenarioVariable>()
            .add_message::<openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables>()
            .add_systems(Update, profile_scenario_variable_application::apply_profile_scenario_variable_changes.in_set(GameSet::Intent))
            .add_message::<RespondToScenarioChallengeRequest>()
            .add_message::<challenge_offer_types::ScenarioChallengePanelRequest>()
            .add_message::<ScenarioChallengeAccepted>()
            .add_message::<ScenarioChallengeAcceptanceRejected>()
            .add_message::<CreateScenarioChallengeOfferFromLuaSourcePathRequest>()
            .add_message::<ScenarioTerminalResult>()
            .add_message::<ScenarioObjectiveStatusChanged>()
            .add_message::<openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest>()
            .add_message::<scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest>()
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
                    scenario_photo_challenge_selection::select_requested_current_photo_challenge_sets,
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    scenario_rule_visibility::show_requested_scenario_rules,
                    scenario_ui_modification_presentation::
                        apply_mode_scenario_ui_modifications_to_projected_nodes,
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
                    evaluate_challenge_manager_and_validate_one_picked_challenge
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
    worlds: Query<
        &openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity,
    >,
) -> bool {
    selected.is_some()
        || worlds.iter().any(|world| {
            world.mode == openzt2_world_state::game_session_types::WorldSessionMode::Challenge
        })
}
