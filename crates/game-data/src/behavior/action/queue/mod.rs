//! Authored queue approach and waiting policy; live membership belongs to ECS.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorQueueWaitAction {
    /// Optional container override; otherwise use the enclosing task's tag.
    pub container: Option<crate::AssetId>,
    /// Behavior repeated while capacity is unavailable.
    pub waiting_behavior_set: crate::AssetId,
    /// Lower bound of the one-time waiting deadline draw.
    pub minimum_wait: std::time::Duration,
    /// Upper bound of the one-time waiting deadline draw.
    pub maximum_wait: std::time::Duration,
}
