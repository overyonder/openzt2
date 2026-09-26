use std::{collections::BTreeMap, path::Path};

use bevy::prelude::*;

use super::lua_script_asset_loading::LuaScriptAsset;

struct IndexedLuaScriptAssetHandle {
    path: String,
    handle: Handle<LuaScriptAsset>,
}

#[derive(Resource)]
pub(crate) struct LuaScriptArchivePathIndex {
    archives: crate::asset_source::AssetArchives,
    indexed_archive_revision: Option<u64>,
    handles_by_normalized_path: BTreeMap<String, IndexedLuaScriptAssetHandle>,
}

impl FromWorld for LuaScriptArchivePathIndex {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
            indexed_archive_revision: None,
            handles_by_normalized_path: BTreeMap::new(),
        }
    }
}

impl LuaScriptArchivePathIndex {
    pub(crate) fn has_pending_script_loads(
        &self,
        asset_server: &AssetServer,
        assets: &Assets<LuaScriptAsset>,
    ) -> bool {
        self.indexed_archive_revision.is_none()
            || self.handles_by_normalized_path.values().any(|script| {
                !assets.contains(script.handle.id())
                    && !matches!(
                        asset_server.load_state(script.handle.id()),
                        bevy::asset::LoadState::Failed(_)
                    )
            })
    }

    pub(crate) fn resolve<'a>(
        &'a self,
        referring_script_path: &str,
        requested_script_reference: &str,
    ) -> Option<(&'a str, &'a Handle<LuaScriptAsset>)> {
        let resolved_path = self.archives.resolve_script_reference(
            Path::new(referring_script_path),
            requested_script_reference,
        )?;
        let indexed_script = self
            .handles_by_normalized_path
            .get(&normalized_lua_script_archive_path(&resolved_path))?;
        Some((&indexed_script.path, &indexed_script.handle))
    }
}

pub(super) fn refresh_lua_script_archive_path_index_after_archive_revision(
    asset_server: Res<AssetServer>,
    mut lua_script_archive_path_index: ResMut<LuaScriptArchivePathIndex>,
) {
    let (archive_revision, resolved_archive_paths) =
        lua_script_archive_path_index.archives.resolved_paths();
    if lua_script_archive_path_index.indexed_archive_revision == Some(archive_revision) {
        return;
    }
    lua_script_archive_path_index.handles_by_normalized_path = resolved_archive_paths
        .iter()
        .filter(|path| path_has_lua_script_asset_extension(path))
        .map(|path| {
            let path = path.to_string_lossy().replace('\\', "/");
            (
                normalized_lua_script_archive_path(Path::new(&path)),
                IndexedLuaScriptAssetHandle {
                    handle: asset_server.load(path.clone()),
                    path,
                },
            )
        })
        .collect();
    lua_script_archive_path_index.indexed_archive_revision = Some(archive_revision);
}

fn path_has_lua_script_asset_extension(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("lua")
            || extension.eq_ignore_ascii_case("bin")
            || extension.eq_ignore_ascii_case("luac")
    })
}

fn normalized_lua_script_archive_path(path: &Path) -> String {
    z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(path)
        .to_string_lossy()
        .into_owned()
}
