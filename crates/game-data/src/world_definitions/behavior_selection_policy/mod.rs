//! Source-authored AI candidate scoring controls shared by animal and guest tasks.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BehaviorSelectionPolicy {
    pub enabled: bool,
    pub minimum_score: f32,
    pub outside_range_need_value: f32,
    /// Actor simulation updates before reconsidering a rejected target.
    pub rejected_target_retry_interval: u32,
    pub rejected_target_retry_interval_cap: u32,
}
