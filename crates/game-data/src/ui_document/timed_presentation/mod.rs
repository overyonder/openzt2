use super::action::presentation::UiPresentationActionRecord;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiLocalizedCountdownRecord {
    pub text_node: AssetId,
    pub localization_key: AssetId,
    pub start_count: u32,
    pub tick_interval_ms: u32,
    pub completion_target: AssetId,
}

/// One cumulative visual-time threshold and its typed presentation action.
/// Equal thresholds retain source order.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiTimedEventRecord {
    pub due_ms: u32,
    pub action: UiPresentationActionRecord,
}
