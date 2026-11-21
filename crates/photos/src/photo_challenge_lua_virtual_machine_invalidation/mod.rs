use std::collections::BTreeSet;

use bevy::prelude::*;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;

use super::photo_challenge_lua_scoring_types::{PhotoChallengeScorer, PhotoChallengeScript};

pub(super) fn invalidate_changed_photo_challenge_scorers(
    mut scripts: MessageReader<AssetEvent<LuaScriptAsset>>,
    mut scorers: Query<(&PhotoChallengeScript, &mut PhotoChallengeScorer)>,
) {
    let changed = scripts
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            AssetEvent::LoadedWithDependencies { .. } | AssetEvent::Unused { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    if changed.is_empty() {
        return;
    }
    for (script, mut scorer) in &mut scorers {
        if scorer.lua.is_some()
            && (changed.contains(&script.0.id()) || !scorer.modules.is_disjoint(&changed))
        {
            *scorer = PhotoChallengeScorer::default();
        }
    }
}
