use std::collections::BTreeSet;

use bevy::prelude::*;
use mlua::Lua;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;

#[derive(Component, Default)]
pub(super) struct PhotoChallengeScorer {
    pub(super) lua: Option<Lua>,
    pub(super) modules: BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
}
