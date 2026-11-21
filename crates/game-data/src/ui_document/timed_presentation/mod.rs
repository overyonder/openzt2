use super::action::UiActionRecord;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// One cumulative visual-time threshold and its typed UI action.
/// Equal thresholds retain source order.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiTimedEventRecord {
    pub due: Duration,
    pub action: UiTimedEventAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiTimedEventAction {
    Ui(UiActionRecord),
    ScenarioScript(String),
}
