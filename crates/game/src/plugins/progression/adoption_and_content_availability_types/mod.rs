use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalAdoptionAvailabilityPolicy {
    pub enabled: bool,
    pub multiplier_permille: u16,
}

impl Default for AnimalAdoptionAvailabilityPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            multiplier_permille: 1000,
        }
    }
}

/// Scenario-local chance modifier for one normalized adoptable definition.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeciesAdoptionChanceMultiplier {
    pub definition: AssetId,
    pub multiplier_permille: u16,
}

/// Scenario-local content made visible by authored scenario progression.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct ScenarioContentAvailability {
    ids: Vec<AssetId>,
}

impl ScenarioContentAvailability {
    pub fn contains(&self, id: AssetId) -> bool {
        self.ids
            .binary_search_by_key(&id.0, |candidate| candidate.0)
            .is_ok()
    }

    pub fn make_available(&mut self, id: AssetId) {
        match self
            .ids
            .binary_search_by_key(&id.0, |candidate| candidate.0)
        {
            Ok(_) => {}
            Err(index) => self.ids.insert(index, id),
        }
    }
}
