use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ScenarioCampaignCompletionProgress {
    pub campaign: AssetId,
    pub mission: u32,
    pub completed_words: Vec<u64>,
}

impl ScenarioCampaignCompletionProgress {
    pub fn create_for_campaign_mission_count(
        campaign: AssetId,
        mission: u32,
        mission_count: u32,
    ) -> Self {
        Self {
            campaign,
            mission,
            completed_words: vec![0; mission_count.div_ceil(64) as usize],
        }
    }

    pub fn record_completed_mission(&mut self, mission: u32) -> bool {
        let Some(word) = self.completed_words.get_mut((mission / 64) as usize) else {
            return false;
        };
        let mask = 1_u64 << (mission % 64);
        let changed = *word & mask == 0;
        *word |= mask;
        changed
    }

    pub fn mission_has_been_completed(&self, mission: u32) -> bool {
        self.completed_words
            .get((mission / 64) as usize)
            .is_some_and(|word| word & (1_u64 << (mission % 64)) != 0)
    }
}
