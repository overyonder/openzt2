use bevy::prelude::*;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_data::ui_document::action::script::UiScriptActionRecord;

#[derive(Message)]
pub struct RunAuthoredUiScript {
    pub action: Entity,
    pub document_root: Entity,
    pub invocation: UiScriptActionRecord,
    pub handle: Option<Handle<LuaScriptAsset>>,
    pub timed: bool,
}

#[derive(Message)]
pub struct TimedScenarioUiActionCompleted(pub Entity);

/// A requested selection and the authored actions it activated have settled.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioUiSelectionCompleted {
    pub document_root: Entity,
    pub target_node: openzt2_game_data::AssetId,
}

/// The authored UI document currently settling a script selection.
#[derive(Component)]
pub struct PendingScenarioUiSelection(pub ScenarioUiSelectionCompleted);
