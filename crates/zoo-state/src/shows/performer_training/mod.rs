use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Authored proficiency points owned by the animal, independent of show rows.
#[derive(Component, Default)]
pub struct PerformerTraining {
    scores: BTreeMap<AssetId, u16>,
}

impl PerformerTraining {
    pub fn apply_capped_learning(&mut self, trick: AssetId, amount: u16, cap: u16) {
        let score = self.scores.entry(trick).or_default();
        // A lower-cap learning source must not erase proficiency already earned.
        *score = (*score).max(score.saturating_add(amount).min(cap));
    }
}
