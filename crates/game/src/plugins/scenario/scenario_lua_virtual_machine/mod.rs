use std::{cell::RefCell, collections::BTreeSet};

use bevy::prelude::*;
use mlua::{Function, Lua, Value};
use openzt2_game_data::world_scenario::ScenarioScriptBinding;

use crate::assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::lua_script_module_execution::{
    execute_lua_script_module_once_per_virtual_machine,
    install_lua_script_dependency_loading_functions,
};

use super::{
    scenario_lua_game_api::{
        convert_lua_rule_result_to_objective_status,
        install_scenario_game_query_and_command_functions,
    },
    scenario_lua_types::{
        ScenarioLuaQueryFacts, ScenarioLuaScriptResult, ScenarioLuaVirtualMachine,
        ScenarioLuaVirtualMachineContexts,
    },
    scenario_session_types::SelectedScenarioDocument,
};

impl ScenarioLuaVirtualMachineContexts {
    pub(super) fn clear_finished_zoo_session(&mut self) {
        self.virtual_machines_by_scenario_identifier.clear();
    }

    pub(super) fn clear_finished_challenge_rule_state(
        &mut self,
        scenario: openzt2_game_data::AssetId,
    ) -> mlua::Result<()> {
        let Some(context) = self.virtual_machines_by_scenario_identifier.get(&scenario) else {
            return Ok(());
        };
        let globals = context.lua_virtual_machine.globals();
        globals.set("__openzt2_rule_arguments", Value::Nil)?;
        if let Some(state) = globals.get::<Option<mlua::Table>>("__openzt2_scenario_state")? {
            state.set("challenge", Value::Nil)?;
        }
        // Other authored globals, including one-shot challenge eligibility,
        // survive. Only this rule instance's argument/response lifetime ends.
        Ok(())
    }

    pub(super) fn set_challenge_response(
        &mut self,
        scenario: openzt2_game_data::AssetId,
        accepted: bool,
    ) -> mlua::Result<()> {
        let context = self
            .virtual_machines_by_scenario_identifier
            .get(&scenario)
            .ok_or_else(|| mlua::Error::runtime("challenge has no running Lua context"))?;
        let state: mlua::Table = context
            .lua_virtual_machine
            .globals()
            .get("__openzt2_scenario_state")?;
        state.set("challenge", if accepted { "accept" } else { "decline" })
    }

    pub(super) fn invalidate_changed_scenario_documents_and_lua_scripts(
        &mut self,
        changed_scenario_document_asset_identifiers: &BTreeSet<
            bevy::asset::AssetId<WorldScenarioDocumentAsset>,
        >,
        changed_lua_script_asset_identifiers: &BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
    ) {
        self.virtual_machines_by_scenario_identifier
            .retain(|_, scenario_lua_virtual_machine| {
                !changed_scenario_document_asset_identifiers
                    .contains(&scenario_lua_virtual_machine.scenario_document_asset_identifier)
                    && scenario_lua_virtual_machine
                        .loaded_lua_script_asset_identifiers
                        .is_disjoint(changed_lua_script_asset_identifiers)
            });
    }

    pub(super) fn execute_scenario_lua_script_binding(
        &mut self,
        scenario_identifier: openzt2_game_data::AssetId,
        scenario_document_asset_identifier: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
        lua_script_source_path: &str,
        lua_script_asset_handle: &Handle<LuaScriptAsset>,
        lua_script_assets: &Assets<LuaScriptAsset>,
        lua_script_path_index: &LuaScriptArchivePathIndex,
        scenario_script_binding: &ScenarioScriptBinding,
        scenario_lua_query_facts: ScenarioLuaQueryFacts<'_>,
    ) -> mlua::Result<ScenarioLuaScriptResult> {
        let context = self
            .virtual_machines_by_scenario_identifier
            .entry(scenario_identifier)
            .or_insert_with(|| ScenarioLuaVirtualMachine {
                lua_virtual_machine: Lua::new(),
                scenario_document_asset_identifier,
                loaded_lua_script_asset_identifiers: BTreeSet::new(),
            });
        if context.scenario_document_asset_identifier != scenario_document_asset_identifier {
            *context = ScenarioLuaVirtualMachine {
                lua_virtual_machine: Lua::new(),
                scenario_document_asset_identifier,
                loaded_lua_script_asset_identifiers: BTreeSet::new(),
            };
        }
        context.execute_scenario_lua_script_binding(
            lua_script_source_path,
            lua_script_asset_handle,
            lua_script_assets,
            lua_script_path_index,
            scenario_script_binding,
            scenario_lua_query_facts,
        )
    }
}

impl ScenarioLuaVirtualMachine {
    fn execute_scenario_lua_script_binding(
        &mut self,
        lua_script_source_path: &str,
        lua_script_asset_handle: &Handle<LuaScriptAsset>,
        lua_script_assets: &Assets<LuaScriptAsset>,
        lua_script_path_index: &LuaScriptArchivePathIndex,
        scenario_script_binding: &ScenarioScriptBinding,
        scenario_lua_query_facts: ScenarioLuaQueryFacts<'_>,
    ) -> mlua::Result<ScenarioLuaScriptResult> {
        let requested_economy_operations = RefCell::new(Vec::new());
        let requested_award_point_adjustments = RefCell::new(Vec::new());
        let requested_challenge_offer_source_paths = RefCell::new(Vec::new());
        let requested_challenge_panel = RefCell::new(None);
        let loaded_lua_script_asset_identifiers = RefCell::new(std::mem::take(
            &mut self.loaded_lua_script_asset_identifiers,
        ));
        let active_lua_script_source_path_stack = RefCell::new(Vec::new());
        let execution_result = self.lua_virtual_machine.scope(|lua_scope| {
            install_lua_script_dependency_loading_functions(
                &self.lua_virtual_machine,
                lua_scope,
                lua_script_assets,
                lua_script_path_index,
                &loaded_lua_script_asset_identifiers,
                &active_lua_script_source_path_stack,
            )?;
            execute_lua_script_module_once_per_virtual_machine(
                &self.lua_virtual_machine,
                lua_script_source_path,
                lua_script_asset_handle,
                lua_script_assets,
                &loaded_lua_script_asset_identifiers,
                &active_lua_script_source_path_stack,
            )?;
            install_scenario_game_query_and_command_functions(
                &self.lua_virtual_machine,
                lua_scope,
                scenario_lua_query_facts,
                &requested_economy_operations,
                &requested_award_point_adjustments,
                &requested_challenge_offer_source_paths,
                scenario_script_binding.scenario,
                &requested_challenge_panel,
            )?;
            let scenario_lua_function: Function = self
                .lua_virtual_machine
                .globals()
                .get(scenario_script_binding.entry.as_str())?;
            // Each native rule owns the argument table shared by its Evaluate,
            // Success and Failure calls. Authored scripts store fields such as
            // `data.accept` here across ticks; calling with nil loses that state.
            let globals = self.lua_virtual_machine.globals();
            let arguments = globals
                .get::<Option<mlua::Table>>("__openzt2_rule_arguments")?
                .unwrap_or(self.lua_virtual_machine.create_table()?);
            globals.set("__openzt2_rule_arguments", arguments.clone())?;
            let key = self
                .lua_virtual_machine
                .create_string(scenario_script_binding.objective.0)?;
            let argument = arguments
                .get::<Option<mlua::Table>>(key.clone())?
                .unwrap_or(self.lua_virtual_machine.create_table()?);
            arguments.set(key, argument.clone())?;
            let condition_status = convert_lua_rule_result_to_objective_status(
                scenario_lua_function.call::<Value>(argument)?,
            );
            Ok(ScenarioLuaScriptResult {
                condition_status,
                challenge_panel: requested_challenge_panel.borrow_mut().take(),
                economy_operations: requested_economy_operations
                    .borrow_mut()
                    .drain(..)
                    .collect(),
                award_point_adjustments: requested_award_point_adjustments
                    .borrow_mut()
                    .drain(..)
                    .collect(),
                challenge_offer_source_paths: requested_challenge_offer_source_paths
                    .borrow_mut()
                    .drain(..)
                    .collect(),
            })
        });
        self.loaded_lua_script_asset_identifiers = loaded_lua_script_asset_identifiers.into_inner();
        execution_result
    }
}

pub(super) fn execute_loaded_scenario_lua_script_binding(
    scenario_lua_virtual_machine_contexts: &mut ScenarioLuaVirtualMachineContexts,
    selected_scenario_document: &SelectedScenarioDocument,
    scenario_document_assets: &Assets<WorldScenarioDocumentAsset>,
    lua_script_assets: &Assets<LuaScriptAsset>,
    lua_script_path_index: &LuaScriptArchivePathIndex,
    scenario_script_binding: &ScenarioScriptBinding,
    scenario_lua_query_facts: ScenarioLuaQueryFacts<'_>,
) -> mlua::Result<ScenarioLuaScriptResult> {
    let scenario_document_asset = scenario_document_assets
        .get(&selected_scenario_document.0)
        .ok_or_else(|| mlua::Error::runtime("scenario document is not loaded"))?;
    let (lua_script_source_path, lua_script_asset_handle) = scenario_document_asset
        .script(scenario_script_binding)
        .ok_or_else(|| {
            mlua::Error::runtime(format!(
                "script {} was not resolved",
                scenario_script_binding.script
            ))
        })?;
    scenario_lua_virtual_machine_contexts.execute_scenario_lua_script_binding(
        scenario_script_binding.scenario,
        selected_scenario_document.0.id(),
        lua_script_source_path,
        lua_script_asset_handle,
        lua_script_assets,
        lua_script_path_index,
        scenario_script_binding,
        scenario_lua_query_facts,
    )
}
