use std::collections::BTreeSet;

use bevy::prelude::*;
use openzt2_game_assets::asset_source::ArchiveOverlayChanged;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_data::AssetId;

use super::photo_challenge_lua_scoring_types::{PhotoChallengeScorer, PhotoChallengeScript};

pub(super) fn invalidate_changed_photo_challenge_scorers(
    mut scripts: MessageReader<AssetEvent<LuaScriptAsset>>,
    mut overlay: MessageReader<ArchiveOverlayChanged>,
    server: Res<AssetServer>,
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
    let changed_paths = changed
        .iter()
        .filter_map(|&id| server.get_path(id))
        .map(|path| AssetId::from_virtual_path(&path.path().to_string_lossy()))
        .chain(
            overlay
                .read()
                .flat_map(|event| &event.changes)
                .map(|entry| {
                    AssetId::from_virtual_path(&entry.normalized_asset_path.to_string_lossy())
                }),
        )
        .collect::<BTreeSet<_>>();
    if changed.is_empty() && changed_paths.is_empty() {
        return;
    }
    for (script, mut scorer) in &mut scorers {
        if scorer.lua.is_some()
            && (changed.contains(&script.0.id()) || !scorer.modules.is_disjoint(&changed_paths))
        {
            *scorer = PhotoChallengeScorer::default();
        }
    }
}
