use std::sync::Arc;

use bevy::prelude::*;

#[derive(Message)]
pub struct RunTimedScenarioUiAction {
    pub action: Entity,
    pub document_root: Entity,
    pub command: Arc<str>,
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
