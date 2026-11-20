use std::collections::BTreeSet;

use bevy::prelude::*;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;

use super::photo_challenge_lua_scoring_types::PhotoChallengeScorer;

pub(super) fn invalidate_changed_photo_challenge_scorers(
    mut documents: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut scripts: MessageReader<AssetEvent<LuaScriptAsset>>,
    mut scorers: Query<(Ref<PhotoChallenge>, &mut PhotoChallengeScorer)>,
) {
    let documents = documents
        .read()
        .filter_map(changed_asset)
        .collect::<BTreeSet<_>>();
    let scripts = scripts
        .read()
        .filter_map(changed_asset)
        .collect::<BTreeSet<_>>();
    for (challenge, mut scorer) in &mut scorers {
        if scorer.lua.is_some()
            && (challenge.is_changed()
                || documents.contains(&challenge.source.id())
                || !scorer.modules.is_disjoint(&scripts))
        {
            *scorer = PhotoChallengeScorer::default();
        }
    }
}

fn changed_asset<A: Asset>(event: &AssetEvent<A>) -> Option<bevy::asset::AssetId<A>> {
    match *event {
        AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
            Some(id)
        }
        AssetEvent::LoadedWithDependencies { .. } | AssetEvent::Unused { .. } => None,
    }
}
