use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::ObjectiveStatusFilter;

/// Typed presentation role carried by one authored scenario-objective row
/// variant. The scenario plugin supplies live status; UI projection owns only
/// this immutable visual classification.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiScenarioObjectiveStatusVisualClassification(pub ObjectiveStatusFilter);
