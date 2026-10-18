use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::AssetId;

/// A finite-lived token delivered to the executing subject or its task
/// target, without a payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorSelfTokenDelivery {
    pub token: AssetId,
    pub delay: Duration,
    pub lifetime: Duration,
    pub chance_percent: f32,
    #[serde(default)]
    pub recipient: BehaviorTokenRecipient,
    /// The recipient abandons its current task to consider the token at once.
    #[serde(default)]
    pub reconsider: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorTokenRecipient {
    #[default]
    Subject,
    Target,
}
