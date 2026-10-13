use serde::{Deserialize, Serialize};

/// Authored event policy. Optional times are seconds, not animation completion
/// or a successful trick result.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorShowEventAction {
    pub name: crate::AssetId,
    pub priority: u16,
    pub interrupt: bool,
    pub expire_with_task: bool,
    pub time_in_seconds: Option<f32>,
    pub timeout_seconds: Option<f32>,
}
