use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioAwardPointTotal(pub u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdjustScenarioAwardPointTotalRequest {
    pub(crate) delta: i32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EarnedProgressionAward {
    pub definition: AssetId,
    pub earned_tick: u64,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ProgressionAwardPresentationAcknowledged;

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
pub(crate) struct ProgressionAwardGrantCandidate {
    pub definition: AssetId,
}

#[derive(Component)]
pub(crate) struct ProgressionAwardConditionStorageInitialized;
