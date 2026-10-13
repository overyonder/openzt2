//! Persistent state for individual photo challenges and grouped challenge sets.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub struct PhotoChallengesInitializedForScenarioRevision(pub u64);

impl PhotoChallengesInitializedForScenarioRevision {
    pub fn matches_revision(&self, revision: u64) -> bool {
        self.0 == revision
    }
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PhotoChallenge {
    pub id: AssetId,
    pub source: Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
    pub row: u32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhotoChallengeProgress {
    pub completed: bool,
    pub completed_by: Option<Entity>,
    pub rating_stars: u8,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PhotoChallengeSet {
    pub id: AssetId,
    pub source: Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
    pub row: u32,
}

/// The photo challenge set a scenario selected with
/// `ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE`; at most one per world.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CurrentPhotoChallengeSet;
