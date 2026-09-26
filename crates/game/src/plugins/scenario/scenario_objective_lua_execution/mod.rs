use bevy::prelude::*;
use openzt2_game_data::world_scenario::ScenarioScriptPhase;

use crate::assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::{
    challenge_offer_types::CreateScenarioChallengeOfferFromLuaSourcePathRequest,
    scenario_lua_command_publication::publish_commands_produced_by_scenario_lua_script,
    scenario_lua_query_fact_collection::count_live_world_entities_by_definition,
    scenario_lua_types::{ScenarioLuaQueryFacts, ScenarioLuaVirtualMachineContexts},
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
    cash: Res<crate::plugins::economy::zoo_cash_types::ZooCash>,
    admission: Res<crate::plugins::economy::guest_admission_types::AdmissionPrice>,
    fame: Res<crate::plugins::progression::fame_types::Fame>,
    clock: Option<Res<crate::plugins::simulation_time::simulation_clock_types::ZooClock>>,
    calendar: Option<Res<crate::plugins::simulation_time::simulation_clock_types::ZooCalendar>>,
    definitions: Query<&crate::plugins::world_spawn::world_membership_types::DefinitionId>,
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
    ),
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let definition_counts = count_live_world_entities_by_definition(&definitions);
    let queries = ScenarioLuaQueryFacts {
        zoo_cash: &cash,
        guest_admission_price: &admission,
        zoo_fame: &fame,
        zoo_clock: clock.as_deref(),
        zoo_calendar: calendar.as_deref(),
        live_world_entity_counts_by_definition_identifier: &definition_counts,
    };
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
                Ok(result) if result.condition_status != ScenarioObjectiveStatus::Active => {
                    let next_status = result.condition_status;
                    publish_commands_produced_by_scenario_lua_script(
                        entity,
                        result,
                        &mut outputs.0,
                        &mut outputs.1,
                        &mut outputs.2,
                        &mut outputs.3,
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
    cash: Res<crate::plugins::economy::zoo_cash_types::ZooCash>,
    admission: Res<crate::plugins::economy::guest_admission_types::AdmissionPrice>,
    fame: Res<crate::plugins::progression::fame_types::Fame>,
    clock: Option<Res<crate::plugins::simulation_time::simulation_clock_types::ZooClock>>,
    calendar: Option<Res<crate::plugins::simulation_time::simulation_clock_types::ZooCalendar>>,
    definitions: Query<&crate::plugins::world_spawn::world_membership_types::DefinitionId>,
    objectives: Query<&ScenarioObjective>,
    mut changed: MessageReader<ScenarioObjectiveStatusChanged>,
    mut outputs: (
        MessageWriter<crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyCommand>,
        MessageWriter<crate::plugins::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
    ),
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let definition_counts = count_live_world_entities_by_definition(&definitions);
    let queries = ScenarioLuaQueryFacts {
        zoo_cash: &cash,
        guest_admission_price: &admission,
        zoo_fame: &fame,
        zoo_clock: clock.as_deref(),
        zoo_calendar: calendar.as_deref(),
        live_world_entity_counts_by_definition_identifier: &definition_counts,
    };
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
                Ok(result) => publish_commands_produced_by_scenario_lua_script(
                    event.objective,
                    result,
                    &mut outputs.0,
                    &mut outputs.1,
                    &mut outputs.2,
                    &mut outputs.3,
                ),
                Err(error) => {
                    error!(entry = %binding.entry, %error, "scenario Lua terminal call failed")
                }
            }
        }
    }
}
