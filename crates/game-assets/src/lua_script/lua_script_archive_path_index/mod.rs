use std::{collections::BTreeMap, path::Path, sync::Arc};

use bevy::prelude::*;

use super::lua_script_asset_loading::LuaScriptAsset;
use crate::asset_source::{ArchiveCatalogue, AssetArchives};

struct IndexedLuaScriptAssetHandle {
    path: String,
    handle: Handle<LuaScriptAsset>,
}

#[derive(Resource)]
pub struct LuaScriptArchivePathIndex {
    archives: crate::asset_source::AssetArchives,
    discovered: bool,
    handles_by_normalized_path: BTreeMap<String, IndexedLuaScriptAssetHandle>,
}

impl FromWorld for LuaScriptArchivePathIndex {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
            discovered: false,
            handles_by_normalized_path: BTreeMap::new(),
        }
    }
}

impl LuaScriptArchivePathIndex {
    pub fn has_pending_script_loads(
        &self,
        asset_server: &AssetServer,
        assets: &Assets<LuaScriptAsset>,
    ) -> bool {
        !self.discovered
            || self.handles_by_normalized_path.values().any(|script| {
                !assets.contains(script.handle.id())
                    && !matches!(
                        asset_server.load_state(script.handle.id()),
                        bevy::asset::LoadState::Failed(_)
                    )
            })
    }

    pub fn resolve<'a>(
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

pub(super) fn discover_lua_script_archive_paths(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut index: ResMut<LuaScriptArchivePathIndex>,
    mut previous: Local<Option<Arc<[z2f::ResolvedArchiveEntry]>>>,
) {
    let sources = archives.catalogue_sources(ArchiveCatalogue::Scripts);
    if previous
        .as_ref()
        .is_some_and(|previous| Arc::ptr_eq(previous, &sources))
    {
        return;
    }
    index.handles_by_normalized_path = sources
        .iter()
        .map(|source| {
            let path = source.normalized_asset_path.to_string_lossy().into_owned();
            let handle = server.load(source.normalized_asset_path.clone());
            (path.clone(), IndexedLuaScriptAssetHandle { path, handle })
        })
        .collect();
    index.discovered = true;
    *previous = Some(sources);
}

pub(crate) fn path_has_lua_script_asset_extension(path: &Path) -> bool {
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
