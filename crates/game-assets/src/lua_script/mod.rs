//! Lua source and normalized-bytecode asset loading and virtual source resolution.

pub mod lua_script_asset_loading;
pub mod lua_script_sources;

use bevy::prelude::*;

use lua_script_asset_loading::register_lua_script_asset_and_loader;
use lua_script_sources::LuaScriptSources;

pub struct LuaScriptAssetLoadingPlugin;

impl Plugin for LuaScriptAssetLoadingPlugin {
    fn build(&self, application: &mut App) {
        register_lua_script_asset_and_loader(application);
        application.init_resource::<LuaScriptSources>();
    }
}
pub mod lua_script_module_execution;
pub mod lua_value_conversion;
