//! Source-authored AI candidate scoring controls shared by animal and guest tasks.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BehaviorSelectionPolicy {
    pub enabled: bool,
    pub minimum_score: f32,
    pub outside_range_need_value: f32,
}
