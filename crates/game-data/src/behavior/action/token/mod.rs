use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::AssetId;

/// A finite-lived token delivered to the executing subject, without a payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorSelfTokenDelivery {
    pub token: AssetId,
    pub delay: Duration,
    pub lifetime: Duration,
    pub chance_percent: f32,
}
