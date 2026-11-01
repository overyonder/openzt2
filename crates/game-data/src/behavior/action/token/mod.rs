use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::AssetId;

/// A payload-free token delivered to the executing subject or its task target.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorSelfTokenDelivery {
    pub token: AssetId,
    pub delay: Duration,
    /// `None` keeps the token until a task consumes it: an absent `Timeout`
    /// or the authored `-1` sentinel.
    pub lifetime: Option<Duration>,
    pub chance_percent: f32,
    #[serde(default)]
    pub recipient: BehaviorTokenRecipient,
    /// The recipient drops its current task and chooses again at once, so
    /// the token's task can take over without the old task's failure branch.
    #[serde(default)]
    pub reconsider: bool,
    /// `GiveTo` naming an entity type, with `Radius`: replaces `recipient`.
    #[serde(default)]
    pub nearby: Option<BehaviorNearbyTokenRecipients>,
    /// `OnlyOne`: nothing is delivered while the recipient holds this token.
    #[serde(default)]
    pub only_one: bool,
}

/// Every live animal of `entity_type` within `radius_m` of the subject, each
/// receiving the token with `chance_percent` (`ChanceInRadius`), as a disease
/// spreads to nearby animals.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct BehaviorNearbyTokenRecipients {
    pub entity_type: AssetId,
    pub radius_m: f32,
    pub chance_percent: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorTokenRecipient {
    #[default]
    Subject,
    Target,
}
