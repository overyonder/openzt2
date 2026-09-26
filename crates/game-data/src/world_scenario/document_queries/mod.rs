use crate::AssetId;

use super::{
    PhotoChallengeRule, PhotoChallengeSetRecord, ScenarioCampaignRecord, ScenarioDefinitionRecord,
    ScenarioObjectiveRecord, StartingZooRecord, WorldMapRecord, WorldScenarioDocument,
};

impl WorldScenarioDocument {
    #[must_use]
    pub fn find_world_map(&self, map_id: AssetId) -> Option<&WorldMapRecord> {
        self.maps.iter().find(|map| map.id == map_id)
    }

    #[must_use]
    pub fn find_starting_zoo(&self, starting_zoo_id: AssetId) -> Option<&StartingZooRecord> {
        self.starting_zoos
            .iter()
            .find(|starting_zoo| starting_zoo.id == starting_zoo_id)
    }

    #[must_use]
    pub fn find_scenario_record(&self, scenario_id: AssetId) -> Option<&ScenarioDefinitionRecord> {
        self.scenarios
            .iter()
            .find(|scenario| scenario.id == scenario_id)
    }

    #[must_use]
    pub fn find_campaign_record(&self, campaign_id: AssetId) -> Option<&ScenarioCampaignRecord> {
        self.campaigns
            .iter()
            .find(|campaign| campaign.id == campaign_id)
    }

    #[must_use]
    pub fn find_photo_challenge_rule(
        &self,
        photo_challenge_id: AssetId,
    ) -> Option<&PhotoChallengeRule> {
        self.photo_challenges
            .iter()
            .find(|photo_challenge| photo_challenge.id == photo_challenge_id)
    }

    #[must_use]
    pub fn find_photo_challenge_set(
        &self,
        photo_challenge_set_id: AssetId,
    ) -> Option<&PhotoChallengeSetRecord> {
        self.photo_challenge_sets
            .iter()
            .find(|photo_challenge_set| photo_challenge_set.id == photo_challenge_set_id)
    }

    #[must_use]
    pub fn find_scenario_objective(
        &self,
        scenario_id: AssetId,
        objective_index: u32,
    ) -> Option<&ScenarioObjectiveRecord> {
        self.find_scenario_record(scenario_id)?
            .objectives
            .get(objective_index as usize)
    }
}
