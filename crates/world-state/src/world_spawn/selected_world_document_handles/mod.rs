use bevy::prelude::*;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

#[derive(Component, Debug, Clone)]
pub struct SelectedWorldDocumentHandles {
    pub map: Handle<WorldScenarioDocumentAsset>,
    pub start: Handle<WorldScenarioDocumentAsset>,
}
