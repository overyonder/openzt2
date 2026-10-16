use bevy::prelude::*;
use openzt2_game_data::world_scenario::ScenarioScriptPhase;

use openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjective;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatusChanged;
use openzt2_zoo_state::scenario::scenario_session_types::SelectedScenarioDocument;

use super::{
    challenge_offer_types::CreateScenarioChallengeOfferFromLuaSourcePathRequest,
    scenario_lua_command_publication::publish_commands_produced_by_scenario_lua_script,
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_binding,
};

pub(super) fn evaluate_active_scenario_objective_lua_condition_scripts(
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    fact_sources: ScenarioLuaQueryFactSources,
    spawn_sources: super::scenario_lua_entity_commands::ScenarioAnimalSpawnSources,
    mut objectives: Query<(
        Entity,
        &ScenarioObjective,
        &mut ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
    )>,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
    mut outputs: (
        MessageWriter<openzt2_zoo_state::economy::scenario_economy_command_types::ScenarioEconomyCommand>,
        MessageWriter<openzt2_zoo_state::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
        MessageWriter<openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest>,
        MessageWriter<openzt2_zoo_state::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest>,
        MessageWriter<super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest>,
        MessageWriter<openzt2_zoo_state::progression::profile_scenario_variables::SetProfileScenarioVariable>,
        MessageWriter<openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables>,
        MessageWriter<super::scenario_lua_ui::ScenarioUiCommand>,
        MessageWriter<super::scenario_lua_entity_commands::ScenarioEntityCommand>,
    ),
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let spawn_sources = std::cell::RefCell::new(spawn_sources);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawn_sources
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let mut queries = fact_sources.borrow_scenario_lua_query_facts();
    queries.spawn_animal = Some(&spawn_animal);
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
                        &mut outputs.7,
                        &mut outputs.8,
                        &mut outputs.9,
                        &mut outputs.10,
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
                        &mut outputs.7,
                        &mut outputs.8,
                        &mut outputs.9,
                        &mut outputs.10,
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
                    &mut outputs.7,
                    &mut outputs.8,
                    &mut outputs.9,
                    &mut outputs.10,
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
    spawn_sources: super::scenario_lua_entity_commands::ScenarioAnimalSpawnSources,
    objectives: Query<&ScenarioObjective>,
    mut changed: MessageReader<ScenarioObjectiveStatusChanged>,
    mut outputs: (
        MessageWriter<openzt2_zoo_state::economy::scenario_economy_command_types::ScenarioEconomyCommand>,
        MessageWriter<openzt2_zoo_state::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
        MessageWriter<openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest>,
        MessageWriter<openzt2_zoo_state::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest>,
        MessageWriter<super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest>,
        MessageWriter<openzt2_zoo_state::progression::profile_scenario_variables::SetProfileScenarioVariable>,
        MessageWriter<openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables>,
        MessageWriter<super::scenario_lua_ui::ScenarioUiCommand>,
        MessageWriter<super::scenario_lua_entity_commands::ScenarioEntityCommand>,
    ),
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let spawn_sources = std::cell::RefCell::new(spawn_sources);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawn_sources
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let mut queries = fact_sources.borrow_scenario_lua_query_facts();
    queries.spawn_animal = Some(&spawn_animal);
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
                        &mut outputs.7,
                        &mut outputs.8,
                        &mut outputs.9,
                        &mut outputs.10,
                    );
                }
                Err(error) => {
                    error!(entry = %binding.entry, %error, "scenario Lua terminal call failed")
                }
            }
        }
    }
}
