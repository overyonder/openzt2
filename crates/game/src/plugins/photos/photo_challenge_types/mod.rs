//! Persistent state for individual photo challenges and grouped challenge sets.

use bevy::prelude::*;
use openzt2_game_data::{world_scenario::PHOTO_DISTINCT_SUBJECT_CAPACITY, AssetId};

#[derive(Component)]
pub(crate) struct PhotoChallengesInitializedForScenarioRevision(pub(super) u64);

impl PhotoChallengesInitializedForScenarioRevision {
    pub(crate) fn matches_revision(&self, revision: u64) -> bool {
        self.0 == revision
    }
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhotoChallenge {
    pub(crate) id: AssetId,
    pub(crate) source: Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
    pub(crate) row: u32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PhotoChallengeProgress {
    pub(crate) completed: bool,
    pub(crate) completed_by: Option<Entity>,
    pub(crate) rating_stars: u8,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhotoChallengeSet {
    pub(crate) id: AssetId,
    pub(crate) source: Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
    pub(crate) row: u32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PhotoChallengeSetProgress {
    pub(crate) completed_members: u16,
    pub(crate) rating_stars: u16,
    pub(crate) selected_subjects: [AssetId; PHOTO_DISTINCT_SUBJECT_CAPACITY],
    pub(crate) selected_subject_count: u8,
}

/// The photo challenge set a scenario selected with
/// `ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE`; at most one per world.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CurrentPhotoChallengeSet;
