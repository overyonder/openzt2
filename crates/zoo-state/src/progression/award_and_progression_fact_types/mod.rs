use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioAwardPointTotal(pub u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustScenarioAwardPointTotalRequest {
    pub delta: i32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EarnedProgressionAward {
    pub definition: AssetId,
    pub earned_tick: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressionAward {
    pub definition: AssetId,
    pub state: ProgressionAwardState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionAwardState {
    Evaluating,
    Ready,
    Earned,
}

#[derive(Component)]
#[relationship(relationship_target = ProgressionAwardConditions)]
pub struct ProgressionAwardConditionOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ProgressionAwardConditionOf, linked_spawn)]
pub struct ProgressionAwardConditions(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredAwardConditionIndex(pub u32);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgressionAwardPresentationAcknowledged;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressionAwardConditionDurationProgress {
    pub award: AssetId,
    pub condition_index: u32,
    pub satisfied_ticks: u64,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionFactChanged {
    Fame,
    Rating,
    Unlock(AssetId),
    Research(AssetId),
    Award(AssetId),
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressionAwardGranted {
    pub award: Entity,
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressionAwardGrantCandidate {
    pub award: Entity,
}

#[derive(Component)]
pub struct ProgressionAwardConditionStorageInitialized;
