use bevy::prelude::*;
use openzt2_game_data::world_scenario::ScenarioScriptPhase;

use openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::{
    challenge_offer_types::CreateScenarioChallengeOfferFromLuaSourcePathRequest,
    scenario_lua_command_publication::publish_commands_produced_by_scenario_lua_script,
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_binding,
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveProgress, ScenarioObjectiveStatus,
        ScenarioObjectiveStatusChanged,
    },
    scenario_session_types::SelectedScenarioDocument,
};

pub(super) fn evaluate_active_scenario_objective_lua_condition_scripts(
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    fact_sources: ScenarioLuaQueryFactSources,
    mut objectives: Query<(
        Entity,
        &ScenarioObjective,
        &mut ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
    )>,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
    mut outputs: (
        MessageWriter<crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyCommand>,
        MessageWriter<crate::plugins::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
        MessageWriter<super::scenario_objective_types::ShowScenarioRuleRequest>,
        MessageWriter<crate::plugins::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest>,
        MessageWriter<super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest>,
    ),
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let definition_counts = fact_sources.count_live_world_entities_by_definition();
    let queries = fact_sources.borrow_scenario_lua_query_facts(&definition_counts);
    for (entity, objective, mut status, progress) in &mut objectives {
        if *status != ScenarioObjectiveStatus::Active {
            continue;
        }
        let Some(handle) = scenarios.handle_for_scenario(objective.scenario) else {
            continue;
        };
        let Some(asset) = scenario_assets.get(handle) else {
            continue;
        };
        let Some(record) = asset
            .document
            .find_scenario_objective(objective.scenario, objective.record_index)
        else {
            continue;
        };
        let source = SelectedScenarioDocument(handle.clone());
        for binding in asset
            .document
            .scenario_script_bindings
            .iter()
            .filter(|binding| {
                (binding.scenario, binding.objective, binding.phase)
                    == (objective.scenario, record.id, ScenarioScriptPhase::Evaluate)
            })
        {
            match execute_loaded_scenario_lua_script_binding(
                &mut runtime,
                &source,
                &scenario_assets,
                &scripts,
                &script_index,
                binding,
                queries,
            ) {
                Ok(result) if result.failure.is_some() => {
                    if let Some(error) = &result.failure {
                        error!(entry = %binding.entry, %error, "scenario Lua call failed");
                    }
                    publish_commands_produced_by_scenario_lua_script(
                        entity,
                        result,
                        &mut outputs.0,
                        &mut outputs.1,
                        &mut outputs.2,
                        &mut outputs.3,
                        &mut outputs.4,
                        &mut outputs.5,
                        &mut outputs.6,
                    );
                }
                Ok(result) if result.condition_status != ScenarioObjectiveStatus::Active => {
                    let next_status = result.condition_status;
                    publish_commands_produced_by_scenario_lua_script(
                        entity,
                        result,
                        &mut outputs.0,
                        &mut outputs.1,
                        &mut outputs.2,
                        &mut outputs.3,
                        &mut outputs.4,
                        &mut outputs.5,
                        &mut outputs.6,
                    );
                    *status = next_status;
                    changed.write(ScenarioObjectiveStatusChanged {
                        objective: entity,
                        status: *status,
                        current: progress.current,
                        target: progress.target,
                    });
                }
                Ok(result) => publish_commands_produced_by_scenario_lua_script(
                    entity,
                    result,
                    &mut outputs.0,
                    &mut outputs.1,
                    &mut outputs.2,
                    &mut outputs.3,
                    &mut outputs.4,
                    &mut outputs.5,
                    &mut outputs.6,
                ),
                Err(error) => error!(entry = %binding.entry, %error, "scenario Lua call failed"),
            }
            if *status != ScenarioObjectiveStatus::Active {
                break;
            }
        }
    }
}

pub(super) fn execute_scenario_objective_success_and_failure_lua_scripts(
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    fact_sources: ScenarioLuaQueryFactSources,
    objectives: Query<&ScenarioObjective>,
    mut changed: MessageReader<ScenarioObjectiveStatusChanged>,
    mut outputs: (
        MessageWriter<crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyCommand>,
        MessageWriter<crate::plugins::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
        MessageWriter<super::scenario_objective_types::ShowScenarioRuleRequest>,
        MessageWriter<crate::plugins::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest>,
        MessageWriter<super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest>,
    ),
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let definition_counts = fact_sources.count_live_world_entities_by_definition();
    let queries = fact_sources.borrow_scenario_lua_query_facts(&definition_counts);
    for event in changed.read() {
        let Ok(objective) = objectives.get(event.objective) else {
            continue;
        };
        let phase = match event.status {
            ScenarioObjectiveStatus::Satisfied => ScenarioScriptPhase::Success,
            ScenarioObjectiveStatus::Failed => ScenarioScriptPhase::Failure,
            ScenarioObjectiveStatus::Inactive | ScenarioObjectiveStatus::Active => continue,
        };
        let Some(handle) = scenarios.handle_for_scenario(objective.scenario) else {
            continue;
        };
        let Some(asset) = scenario_assets.get(handle) else {
            continue;
        };
        let source = SelectedScenarioDocument(handle.clone());
        let Some(record) = asset
            .document
            .find_scenario_objective(objective.scenario, objective.record_index)
        else {
            continue;
        };
        for binding in asset
            .document
            .scenario_script_bindings
            .iter()
            .filter(|binding| {
                (binding.scenario, binding.objective, binding.phase)
                    == (objective.scenario, record.id, phase)
            })
        {
            match execute_loaded_scenario_lua_script_binding(
                &mut runtime,
                &source,
                &scenario_assets,
                &scripts,
                &script_index,
                binding,
                queries,
            ) {
                Ok(result) => {
                    if let Some(error) = &result.failure {
                        error!(entry = %binding.entry, %error, "scenario Lua terminal call failed");
                    }
                    publish_commands_produced_by_scenario_lua_script(
                        event.objective,
                        result,
                        &mut outputs.0,
                        &mut outputs.1,
                        &mut outputs.2,
                        &mut outputs.3,
                        &mut outputs.4,
                        &mut outputs.5,
                        &mut outputs.6,
                    );
                }
                Err(error) => {
                    error!(entry = %binding.entry, %error, "scenario Lua terminal call failed")
                }
            }
        }
    }
}
