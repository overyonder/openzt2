//! Persistent state for individual photo challenges and grouped challenge sets.

use bevy::prelude::*;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

#[derive(Component)]
pub struct PhotoChallengeCatalogueBound;

#[derive(Component, Default, PartialEq, Eq)]
pub struct PhotoChallengeCatalogueIndex {
    pub rules: BTreeMap<AssetId, Entity>,
    pub sets: BTreeMap<AssetId, Entity>,
}

#[derive(Component)]
#[relationship(relationship_target = WorldPhotoChallengeEntries)]
pub struct PhotoChallengeEntryOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = PhotoChallengeEntryOf, linked_spawn)]
pub struct WorldPhotoChallengeEntries(Vec<Entity>);

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PhotoChallenge {
    pub id: AssetId,
    pub source: Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhotoChallengeProgress {
    pub completed: bool,
    pub completed_by: Option<Entity>,
    pub rating_half_stars: u8,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PhotoChallengeSet {
    pub id: AssetId,
    pub source: Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>,
}

/// The photo challenge set a scenario selected with
/// `ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE`, stored on the owning world.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CurrentPhotoChallengeSet(pub Option<Entity>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoChallengeCompleted {
    pub photo: Entity,
    pub challenge: AssetId,
    pub subject: AssetId,
    pub rating_half_stars: u8,
}
