use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

pub const MAXIMUM_PROFILE_SCENARIO_VARIABLE_BYTES: usize = 64 * 1024;

/// Authored user variables belong to the selected profile, across zoo sessions.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileScenarioVariables {
    values: BTreeMap<AssetId, String>,
}

#[derive(Message, Clone, Debug)]
pub struct SetProfileScenarioVariable {
    pub key: AssetId,
    pub value: Option<String>,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct PersistProfileScenarioVariables;

impl ProfileScenarioVariables {
    pub fn get(&self, key: AssetId) -> Option<&str> {
        self.values.get(&key).map(String::as_str)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (AssetId, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
    }

    pub fn set(&mut self, key: AssetId, value: Option<&str>) -> Result<bool, &'static str> {
        let Some(value) = value else {
            return Ok(self.values.remove(&key).is_some());
        };
        let encoded_bytes = 4
            + self
                .values
                .iter()
                .filter(|(existing, _)| **existing != key)
                .map(|(_, value)| 18 + value.len())
                .sum::<usize>()
            + 18
            + value.len();
        if value.len() > 1024 || encoded_bytes > MAXIMUM_PROFILE_SCENARIO_VARIABLE_BYTES {
            return Err("profile scenario variable capacity exceeded");
        }
        if self.get(key) == Some(value) {
            return Ok(false);
        }
        self.values.insert(key, value.to_owned());
        Ok(true)
    }

    pub fn scenario_state(
        &self,
        scenario: &openzt2_game_data::world_scenario::CampaignScenarioRecord,
    ) -> openzt2_game_data::world_scenario::CampaignScenarioState {
        use openzt2_game_data::world_scenario::CampaignScenarioState;
        match self.get(scenario.id) {
            Some("completed") => CampaignScenarioState::Completed,
            Some("unlocked") => CampaignScenarioState::Unlocked,
            Some("locked") => CampaignScenarioState::Locked,
            _ => scenario.initial_state,
        }
    }
}
