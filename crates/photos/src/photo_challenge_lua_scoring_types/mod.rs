use std::collections::BTreeSet;

use bevy::prelude::*;
use mlua::Lua;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_data::AssetId;

#[derive(Component, Default)]
pub(super) struct PhotoChallengeScorer {
    pub(super) lua: Option<Lua>,
    pub(super) modules: BTreeSet<AssetId>,
}

#[derive(Component)]
pub(super) struct PhotoChallengeScript(pub(super) Handle<LuaScriptAsset>);

#[derive(Component)]
#[relationship(relationship_target = PhotoRulesUsingScript)]
pub(super) struct PhotoRuleScriptContext(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = PhotoRuleScriptContext)]
pub(super) struct PhotoRulesUsingScript(Vec<Entity>);
