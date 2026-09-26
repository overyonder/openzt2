use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveScenarioSession {
    pub definition: AssetId,
    pub started_tick: u64,
    pub state: ScenarioSessionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioSessionState {
    Running,
    Won,
    Lost,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingScenarioTerminalResult {
    pub scenario: AssetId,
    pub state: ScenarioSessionState,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioTerminalResult {
    pub scenario: AssetId,
    pub state: ScenarioSessionState,
}

#[derive(Resource, Debug, Clone)]
pub(crate) struct SelectedScenarioDocument(
    pub Handle<crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
);
