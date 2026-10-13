//! Authored show-event scoring retained by the watching behavior action.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorWatchShowAction {
    pub events: Vec<ShowEventScoring>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShowEventScoring {
    pub event: crate::AssetId,
    // Missing inputs remain absent; audience registration does not score events.
    pub priority_q16: Option<i32>,
    pub base_score_q16: Option<i32>,
    pub fuzzy_factor_q16: Option<i32>,
    pub favorite_animal_bonus_q16: Option<i32>,
    pub trick_popularity_multiplier_q16: Option<i32>,
    pub show_quality_multiplier_q16: Option<i32>,
    pub per_previous_q16: Option<i32>,
    pub ignore_previous_q16: Option<i32>,
    pub reactions: Vec<ShowEventReaction>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShowEventReaction {
    pub minimum_score_q16: i32,
    pub behavior_set: crate::AssetId,
}
