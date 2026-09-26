//! Lua source and normalized-bytecode asset loading and archive indexing.

pub(crate) mod lua_script_archive_path_index;
pub(crate) mod lua_script_asset_loading;

use bevy::prelude::*;

use lua_script_archive_path_index::{
    refresh_lua_script_archive_path_index_after_archive_revision, LuaScriptArchivePathIndex,
};
use lua_script_asset_loading::register_lua_script_asset_and_loader;

pub(crate) struct LuaScriptAssetLoadingPlugin;

impl Plugin for LuaScriptAssetLoadingPlugin {
    fn build(&self, application: &mut App) {
        register_lua_script_asset_and_loader(application);
        application
            .init_resource::<LuaScriptArchivePathIndex>()
            .add_systems(
                PreUpdate,
                refresh_lua_script_archive_path_index_after_archive_revision,
            );
    }
}
