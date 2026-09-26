use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::scenario_lua_types::ScenarioLuaVirtualMachineContexts;

pub(super) fn invalidate_virtual_machines_affected_by_changed_scenario_or_script_assets(
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    mut scenarios: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut scripts: MessageReader<AssetEvent<LuaScriptAsset>>,
) {
    let documents = scenarios
        .read()
        .map(asset_identifier_from_asset_event)
        .collect::<BTreeSet<_>>();
    let scripts = scripts
        .read()
        .map(asset_identifier_from_asset_event)
        .collect::<BTreeSet<_>>();
    if !documents.is_empty() || !scripts.is_empty() {
        runtime.invalidate_changed_scenario_documents_and_lua_scripts(&documents, &scripts);
    }
}

fn asset_identifier_from_asset_event<A: Asset>(event: &AssetEvent<A>) -> bevy::asset::AssetId<A> {
    match event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Unused { id } => *id,
    }
}
