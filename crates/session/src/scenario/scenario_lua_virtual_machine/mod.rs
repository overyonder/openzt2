use std::{cell::RefCell, collections::BTreeSet};

use bevy::prelude::*;
use mlua::{Function, Lua, Value};
use openzt2_game_data::world_scenario::ScenarioScriptBinding;

use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::lua_script::lua_script_module_execution::execute_lua_script_module_once_per_virtual_machine;
use openzt2_game_assets::lua_script::lua_script_module_execution::install_lua_script_dependency_loading_functions;
use openzt2_game_assets::lua_script::lua_script_sources::LuaScriptSources;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioDocument;

use super::{
    scenario_lua_game_api::{
        convert_lua_rule_result_to_objective_status,
        install_scenario_game_query_and_command_functions,
    },
    scenario_lua_snapshot_types::{
        saved_scenario_action_key, SAVED_SCENARIO_ACTION_ARGUMENTS, SCENARIO_GLOBAL_VARIABLES,
    },
    scenario_lua_types::{
        ScenarioLuaContext, ScenarioLuaQueryFacts, ScenarioLuaScriptResult,
        ScenarioLuaVirtualMachine,
    },
};

impl ScenarioLuaContext {
    pub(super) fn clear_finished_challenge_rule_state(&mut self) -> mlua::Result<()> {
        let Some(context) = &self.0 else {
            return Ok(());
        };
        let globals = context.lua_virtual_machine.globals();
        globals.set(SAVED_SCENARIO_ACTION_ARGUMENTS, Value::Nil)?;
        if let Some(state) = globals.get::<Option<mlua::Table>>(SCENARIO_GLOBAL_VARIABLES)? {
            state.set("challenge", Value::Nil)?;
        }
        Ok(())
    }

    pub(super) fn set_challenge_response(&mut self, accepted: bool) -> mlua::Result<()> {
        let context = self
            .0
            .as_ref()
            .ok_or_else(|| mlua::Error::runtime("challenge has no running Lua context"))?;
        let state: mlua::Table = context
            .lua_virtual_machine
            .globals()
            .get(SCENARIO_GLOBAL_VARIABLES)?;
        state.set("challenge", if accepted { "accept" } else { "decline" })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute(
        &mut self,
        scenario_document_asset_identifier: Option<
            bevy::asset::AssetId<WorldScenarioDocumentAsset>,
        >,
        lua_script_source_path: &str,
        lua_script_asset_handle: &Handle<LuaScriptAsset>,
        lua_script_assets: &Assets<LuaScriptAsset>,
        lua_script_sources: &LuaScriptSources,
        binding: &ScenarioScriptBinding,
        facts: ScenarioLuaQueryFacts<'_>,
        arguments: Option<&[&str]>,
    ) -> mlua::Result<ScenarioLuaScriptResult> {
        if self.0.as_ref().is_some_and(|context| {
            context.scenario_document_asset_identifier != scenario_document_asset_identifier
        }) {
            self.0 = None;
        }
        let context = self.0.get_or_insert_with(|| ScenarioLuaVirtualMachine {
            executed_source_paths: Vec::new(),
            lua_virtual_machine: Lua::new(),
            scenario_document_asset_identifier,
            loaded_lua_script_asset_identifiers: BTreeSet::new(),
        });
        context.execute_scenario_lua_script_binding(
            lua_script_source_path,
            lua_script_asset_handle,
            lua_script_assets,
            lua_script_sources,
            binding,
            facts,
            arguments,
        )
    }
}

impl ScenarioLuaVirtualMachine {
    fn execute_scenario_lua_script_binding(
        &mut self,
        lua_script_source_path: &str,
        lua_script_asset_handle: &Handle<LuaScriptAsset>,
        lua_script_assets: &Assets<LuaScriptAsset>,
        lua_script_sources: &LuaScriptSources,
        scenario_script_binding: &ScenarioScriptBinding,
        scenario_lua_query_facts: ScenarioLuaQueryFacts<'_>,
        authored_arguments: Option<&[&str]>,
    ) -> mlua::Result<ScenarioLuaScriptResult> {
        let requested_award_point_adjustments = RefCell::new(Vec::new());
        let maximum_fame_percent = RefCell::new(None);
        let requested_challenge_offer_source_paths = RefCell::new(Vec::new());
        let shown_rules = RefCell::new(Vec::new());
        let available_content = RefCell::new(Vec::new());
        let ui = RefCell::new(Vec::new());
        let entity_operations =
            RefCell::new(super::scenario_lua_entity_commands::EntityOperations::default());
        let persist_profile_variables = std::cell::Cell::new(false);
        let script = openzt2_game_data::AssetId::from_virtual_path(lua_script_source_path);
        let was_loaded = self.loaded_lua_script_asset_identifiers.contains(&script);
        let loaded_lua_script_asset_identifiers = RefCell::new(std::mem::take(
            &mut self.loaded_lua_script_asset_identifiers,
        ));
        let active_lua_script_source_path_stack = RefCell::new(Vec::new());
        let execution_result = self.lua_virtual_machine.scope(|lua_scope| {
            let call = (|| {
                install_lua_script_dependency_loading_functions(
                    &self.lua_virtual_machine,
                    lua_scope,
                    lua_script_assets,
                    lua_script_sources,
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
                if !self
                    .executed_source_paths
                    .iter()
                    .any(|path| path == lua_script_source_path)
                {
                    self.executed_source_paths
                        .push(lua_script_source_path.to_owned());
                }
                install_scenario_game_query_and_command_functions(
                    &self.lua_virtual_machine,
                    lua_scope,
                    scenario_lua_query_facts,
                    &requested_award_point_adjustments,
                    &maximum_fame_percent,
                    &requested_challenge_offer_source_paths,
                    scenario_script_binding.scenario,
                    &shown_rules,
                    &available_content,
                    &persist_profile_variables,
                    &ui,
                    &entity_operations,
                )?;
                let scenario_lua_function: Function = self
                    .lua_virtual_machine
                    .globals()
                    .get(scenario_script_binding.entry.as_str())?;
                active_lua_script_source_path_stack
                    .borrow_mut()
                    .push(lua_script_source_path.to_owned());
                if let Some(authored_arguments) = authored_arguments {
                    let arguments = authored_arguments
                        .iter()
                        .map(|argument| {
                            self.lua_virtual_machine
                                .create_string(argument)
                                .map(Value::String)
                        })
                        .collect::<mlua::Result<Vec<_>>>()?;
                    return Ok(convert_lua_rule_result_to_objective_status(
                        scenario_lua_function
                            .call::<Value>(mlua::MultiValue::from_vec(arguments))?,
                    ));
                }
                let globals = self.lua_virtual_machine.globals();
                let arguments = globals
                    .get::<Option<mlua::Table>>(SAVED_SCENARIO_ACTION_ARGUMENTS)?
                    .unwrap_or(self.lua_virtual_machine.create_table()?);
                globals.set(SAVED_SCENARIO_ACTION_ARGUMENTS, arguments.clone())?;
                let key = self
                    .lua_virtual_machine
                    .create_string(saved_scenario_action_key(scenario_script_binding).0)?;
                let argument = arguments
                    .raw_get::<Option<mlua::Table>>(key.clone())?
                    .unwrap_or(self.lua_virtual_machine.create_table()?);
                arguments.raw_set(key, argument.clone())?;
                Ok(convert_lua_rule_result_to_objective_status(
                    scenario_lua_function.call::<Value>(argument)?,
                ))
            })();
            // Native calls act as they are made. An error raised later in the
            // same call ends the script without undoing the calls before it.
            let (condition_status, failure) = match call {
                Ok(condition_status) => (condition_status, None),
                Err(failure) => (ScenarioObjectiveStatus::Active, Some(failure)),
            };
            let challenge_panel = super::challenge_offer_presentation::authored_panel_request(
                scenario_script_binding.scenario,
                scenario_lua_query_facts.ui_documents,
                &ui.borrow(),
            );
            Ok(ScenarioLuaScriptResult {
                entities: std::mem::take(&mut *entity_operations.borrow_mut()).finish(),
                persist_profile_variables: persist_profile_variables.get(),
                ui: ui.borrow_mut().drain(..).collect(),
                failure,
                maximum_fame_percent: maximum_fame_percent.borrow_mut().take(),
                available_content: available_content.borrow_mut().drain(..).collect(),
                condition_status,
                shown_rules: shown_rules.borrow_mut().drain(..).collect(),
                challenge_panel,
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
        let loaded = loaded_lua_script_asset_identifiers.into_inner();
        if !was_loaded && loaded.contains(&script) {
            self.executed_source_paths
                .push(lua_script_source_path.to_owned());
        }
        self.loaded_lua_script_asset_identifiers = loaded;
        execution_result
    }
}

pub(super) fn execute_loaded_scenario_lua_script_binding(
    context: &mut ScenarioLuaContext,
    selected_scenario_document: &ScenarioDocument,
    scenario_document_assets: &Assets<WorldScenarioDocumentAsset>,
    lua_script_assets: &Assets<LuaScriptAsset>,
    lua_script_sources: &LuaScriptSources,
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
    context.execute(
        Some(selected_scenario_document.0.id()),
        lua_script_source_path,
        lua_script_asset_handle,
        lua_script_assets,
        lua_script_sources,
        scenario_script_binding,
        scenario_lua_query_facts,
        None,
    )
}
