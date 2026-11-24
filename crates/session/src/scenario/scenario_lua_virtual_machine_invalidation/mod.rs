use std::collections::BTreeSet;

use bevy::prelude::*;

use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::scenario_lua_types::ScenarioLuaContext;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioDocument;

pub(super) fn release_removed_scenario_lua_context(
    removed: On<Remove<ScenarioDocument>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<ScenarioLuaContext>();
}

pub(super) fn invalidate_virtual_machines_affected_by_changed_scenario_or_script_assets(
    mut contexts: Query<&mut ScenarioLuaContext>,
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
    if documents.is_empty() && scripts.is_empty() {
        return;
    }
    for mut context in &mut contexts {
        if context.0.as_ref().is_some_and(|vm| {
            vm.scenario_document_asset_identifier
                .is_some_and(|id| documents.contains(&id))
                || !vm.loaded_lua_script_asset_identifiers.is_disjoint(&scripts)
        }) {
            context.0 = None;
        }
    }
}

fn changed_asset_identifier<A: Asset>(event: &AssetEvent<A>) -> Option<bevy::asset::AssetId<A>> {
    match event {
        AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(*id),
        _ => None,
    }
}
