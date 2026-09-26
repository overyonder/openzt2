//! Two authored behavior sets started together on a subject and its target.

use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorSynchronizedSetsAction {
    pub subject_behavior_set: AssetId,
    pub target_behavior_set: AssetId,
    /// Start both animation clocks at the beginning of their phase.
    pub reset_animation_phase: bool,
}
