use std::collections::BTreeSet;

use bevy::prelude::*;

use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::scenario_lua_types::ScenarioLuaVirtualMachineContexts;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioDocument;

pub(super) fn remove_unloaded_scenario_lua_contexts(
    mut removed: RemovedComponents<ScenarioDocument>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
) {
    for owner in removed.read() {
        runtime.remove_owner(owner);
    }
}

pub(super) fn invalidate_virtual_machines_affected_by_changed_scenario_or_script_assets(
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    mut scenarios: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut scripts: MessageReader<AssetEvent<LuaScriptAsset>>,
) {
    let documents = scenarios
        .read()
        .filter_map(changed_asset_identifier)
        .collect::<BTreeSet<_>>();
    let scripts = scripts
        .read()
        .filter_map(changed_asset_identifier)
        .collect::<BTreeSet<_>>();
    if !documents.is_empty() || !scripts.is_empty() {
        runtime.invalidate_changed_scenario_documents_and_lua_scripts(&documents, &scripts);
    }
}

fn changed_asset_identifier<A: Asset>(event: &AssetEvent<A>) -> Option<bevy::asset::AssetId<A>> {
    match event {
        AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(*id),
        _ => None,
    }
}
