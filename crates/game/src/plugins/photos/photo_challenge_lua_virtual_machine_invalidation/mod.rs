use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::photo_challenge_lua_scoring_types::PhotoChallengeLuaScoringVirtualMachineContexts;

impl PhotoChallengeLuaScoringVirtualMachineContexts {
    fn invalidate_virtual_machines_affected_by_changed_assets(
        &mut self,
        changed_scenario_document_asset_identifiers: &BTreeSet<
            bevy::asset::AssetId<WorldScenarioDocumentAsset>,
        >,
        changed_lua_script_asset_identifiers: &BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
    ) {
        self.virtual_machines_by_challenge_identifier
            .retain(|_, lua_scoring_virtual_machine| {
                !changed_scenario_document_asset_identifiers
                    .contains(&lua_scoring_virtual_machine.scenario_document_asset_identifier)
                    && lua_scoring_virtual_machine
                        .loaded_lua_script_asset_identifiers
                        .is_disjoint(changed_lua_script_asset_identifiers)
            });
    }
}

pub(super) fn invalidate_photo_challenge_scoring_virtual_machines_affected_by_changed_assets(
    mut photo_challenge_lua_scoring_virtual_machine_contexts: NonSendMut<
        PhotoChallengeLuaScoringVirtualMachineContexts,
    >,
    mut changed_scenario_document_assets: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut changed_lua_script_assets: MessageReader<AssetEvent<LuaScriptAsset>>,
) {
    let changed_scenario_document_asset_identifiers = changed_scenario_document_assets
        .read()
        .map(asset_identifier_from_asset_event)
        .collect::<BTreeSet<_>>();
    let changed_lua_script_asset_identifiers = changed_lua_script_assets
        .read()
        .map(asset_identifier_from_asset_event)
        .collect::<BTreeSet<_>>();
    if !changed_scenario_document_asset_identifiers.is_empty()
        || !changed_lua_script_asset_identifiers.is_empty()
    {
        photo_challenge_lua_scoring_virtual_machine_contexts
            .invalidate_virtual_machines_affected_by_changed_assets(
                &changed_scenario_document_asset_identifiers,
                &changed_lua_script_asset_identifiers,
            );
    }
}

fn asset_identifier_from_asset_event<A: Asset>(
    asset_event: &AssetEvent<A>,
) -> bevy::asset::AssetId<A> {
    match asset_event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Unused { id } => *id,
    }
}
