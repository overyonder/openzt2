use std::collections::{BTreeMap, BTreeSet};

use mlua::Lua;

use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

#[derive(Default)]
pub(super) struct PhotoChallengeLuaScoringVirtualMachineContexts {
    pub(super) virtual_machines_by_challenge_identifier:
        BTreeMap<openzt2_game_data::AssetId, PhotoChallengeLuaScoringVirtualMachine>,
}

pub(super) struct PhotoChallengeLuaScoringVirtualMachine {
    pub(super) lua_virtual_machine: Lua,
    pub(super) scenario_document_asset_identifier: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
    pub(super) loaded_lua_script_asset_identifiers: BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
}
