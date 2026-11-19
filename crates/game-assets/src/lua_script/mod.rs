//! Lua source and normalized-bytecode asset loading and archive indexing.

pub mod lua_script_archive_path_index;
pub mod lua_script_asset_loading;

use bevy::prelude::*;

use lua_script_archive_path_index::{discover_lua_script_archive_paths, LuaScriptArchivePathIndex};
use lua_script_asset_loading::register_lua_script_asset_and_loader;

pub struct LuaScriptAssetLoadingPlugin;

impl Plugin for LuaScriptAssetLoadingPlugin {
    fn build(&self, application: &mut App) {
        register_lua_script_asset_and_loader(application);
        application
            .init_resource::<LuaScriptArchivePathIndex>()
            .add_systems(
                PreUpdate,
                discover_lua_script_archive_paths
                    .run_if(resource_changed::<crate::asset_source::AssetArchives>),
            );
    }
}
pub mod lua_script_module_execution;
pub mod lua_value_conversion;
