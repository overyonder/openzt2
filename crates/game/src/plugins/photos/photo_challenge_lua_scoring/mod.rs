use std::{cell::RefCell, collections::BTreeSet};

use bevy::prelude::*;
use mlua::{Function, Lua, Scope, Value};

use crate::{
    assets::lua_script::{
        lua_script_archive_path_index::LuaScriptArchivePathIndex,
        lua_script_asset_loading::LuaScriptAsset,
    },
    plugins::{
        lua_script_module_execution::{
            execute_lua_script_module_once_per_virtual_machine,
            install_lua_script_dependency_loading_functions,
        },
        lua_value_conversion::convert_integral_lua_number_to_i64,
    },
};

use super::{
    photo_capture_types::{
        CapturedPhotoEvidence, CapturedPhotoSemantics, PhotoEvidence, PhotoSemanticEvidence,
    },
    photo_challenge_lua_scoring_types::{
        PhotoChallengeLuaScoringVirtualMachine, PhotoChallengeLuaScoringVirtualMachineContexts,
    },
};

impl PhotoChallengeLuaScoringVirtualMachineContexts {
    pub(super) fn execute_photo_challenge_lua_scoring_function(
        &mut self,
        challenge_identifier: openzt2_game_data::AssetId,
        scenario_document_asset_identifier: bevy::asset::AssetId<
            crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
        >,
        lua_script_source_path: &str,
        lua_script_asset_handle: &Handle<LuaScriptAsset>,
        lua_script_assets: &Assets<LuaScriptAsset>,
        lua_script_path_index: &LuaScriptArchivePathIndex,
        lua_scoring_function_name: &str,
        captured_photo_evidence: &CapturedPhotoEvidence,
        captured_photo_semantics: &CapturedPhotoSemantics,
    ) -> mlua::Result<i32> {
        let lua_scoring_virtual_machine = self
            .virtual_machines_by_challenge_identifier
            .entry(challenge_identifier)
            .or_insert_with(|| PhotoChallengeLuaScoringVirtualMachine {
                lua_virtual_machine: Lua::new(),
                scenario_document_asset_identifier,
                loaded_lua_script_asset_identifiers: BTreeSet::new(),
            });
        if lua_scoring_virtual_machine.scenario_document_asset_identifier
            != scenario_document_asset_identifier
        {
            *lua_scoring_virtual_machine = PhotoChallengeLuaScoringVirtualMachine {
                lua_virtual_machine: Lua::new(),
                scenario_document_asset_identifier,
                loaded_lua_script_asset_identifiers: BTreeSet::new(),
            };
        }
        lua_scoring_virtual_machine.execute_photo_challenge_lua_scoring_function(
            lua_script_source_path,
            lua_script_asset_handle,
            lua_script_assets,
            lua_script_path_index,
            lua_scoring_function_name,
            captured_photo_evidence,
            captured_photo_semantics,
        )
    }
}

impl PhotoChallengeLuaScoringVirtualMachine {
    fn execute_photo_challenge_lua_scoring_function(
        &mut self,
        lua_script_source_path: &str,
        lua_script_asset_handle: &Handle<LuaScriptAsset>,
        lua_script_assets: &Assets<LuaScriptAsset>,
        lua_script_path_index: &LuaScriptArchivePathIndex,
        lua_scoring_function_name: &str,
        captured_photo_evidence: &CapturedPhotoEvidence,
        captured_photo_semantics: &CapturedPhotoSemantics,
    ) -> mlua::Result<i32> {
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
            install_captured_photo_evidence_query_functions(
                &self.lua_virtual_machine,
                lua_scope,
                captured_photo_evidence,
                captured_photo_semantics,
            )?;
            let lua_scoring_function: Function = self
                .lua_virtual_machine
                .globals()
                .get(lua_scoring_function_name)?;
            let lua_score =
                lua_scoring_function.call::<Value>(self.lua_virtual_machine.create_table()?)?;
            convert_lua_photo_score_to_game_score(lua_score)
        });
        self.loaded_lua_script_asset_identifiers = loaded_lua_script_asset_identifiers.into_inner();
        execution_result
    }
}

fn install_captured_photo_evidence_query_functions<'scope, 'env: 'scope>(
    lua_virtual_machine: &Lua,
    lua_scope: &'scope Scope<'scope, 'env>,
    captured_photo_evidence: &'env CapturedPhotoEvidence,
    captured_photo_semantics: &'env CapturedPhotoSemantics,
) -> mlua::Result<()> {
    let lua_global_values = lua_virtual_machine.globals();
    for lua_query_function_name in ["numberOfT1", "countType"] {
        lua_global_values.set(
            lua_query_function_name,
            lua_scope.create_function(move |_, requested_definition_name: String| {
                Ok(find_matching_captured_photo_evidence(
                    &requested_definition_name,
                    captured_photo_evidence,
                    captured_photo_semantics,
                )
                .count())
            })?,
        )?;
    }
    lua_global_values.set(
        "numEntityExistsInWater",
        lua_scope.create_function(move |_, requested_definition_name: String| {
            Ok(find_matching_captured_photo_evidence(
                &requested_definition_name,
                captured_photo_evidence,
                captured_photo_semantics,
            )
            .filter(|(_, semantic_evidence)| {
                semantic_evidence.is_some_and(|semantic_evidence| semantic_evidence.aquatic)
            })
            .count())
        })?,
    )?;
    Ok(())
}

fn find_matching_captured_photo_evidence<'a>(
    requested_definition_name: &'a str,
    captured_photo_evidence: &'a CapturedPhotoEvidence,
    captured_photo_semantics: &'a CapturedPhotoSemantics,
) -> impl Iterator<Item = (&'a PhotoEvidence, Option<&'a PhotoSemanticEvidence>)> {
    let requested_definition_identifier =
        openzt2_game_data::AssetId::from_key(requested_definition_name);
    captured_photo_evidence
        .0
        .iter()
        .filter_map(move |photo_evidence| {
            let semantic_evidence = captured_photo_semantics
                .0
                .iter()
                .find(|candidate| candidate.entity == photo_evidence.entity);
            (photo_evidence.definition == requested_definition_identifier
                || semantic_evidence.is_some_and(|semantic_evidence| {
                    semantic_evidence.animal_variant == Some(requested_definition_identifier)
                }))
            .then_some((photo_evidence, semantic_evidence))
        })
}

fn convert_lua_photo_score_to_game_score(lua_score: Value) -> mlua::Result<i32> {
    let original_game_score = convert_integral_lua_number_to_i64(lua_score)
        .and_then(|lua_integer| i32::try_from(lua_integer).ok())
        .ok_or_else(|| mlua::Error::runtime("photo scorer must return an integer"))?;
    Ok(match original_game_score {
        ..=0 => 0,
        1..=5 => original_game_score * 1000,
        6..=5000 => original_game_score,
        _ => original_game_score / 100,
    }
    .clamp(0, 5000))
}
