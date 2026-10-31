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
    /// `OnlyOne`: nothing is delivered while the recipient holds this token.
    #[serde(default)]
    pub only_one: bool,
    /// `Payload`: the participant the token carries, such as the inviting
    /// female in `t_Reproduce`, which a `fromToken` target resolves to.
    #[serde(default)]
    pub payload: Option<BehaviorTokenRecipient>,
    /// `RetainOnFailure`: a task that claimed the token and failed leaves it
    /// for the next attempt, as a due birth's `t_Pregnant` does.
    #[serde(default)]
    pub retain_on_failure: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorTokenRecipient {
    #[default]
    Subject,
    Target,
}
