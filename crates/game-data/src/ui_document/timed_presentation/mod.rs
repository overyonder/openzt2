use super::action::UiActionRecord;
use serde::{Deserialize, Serialize};

/// One cumulative visual-time threshold and its typed UI action.
/// Equal thresholds retain source order.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiTimedEventRecord {
    pub due_ms: u32,
    pub action: UiActionRecord,
}
