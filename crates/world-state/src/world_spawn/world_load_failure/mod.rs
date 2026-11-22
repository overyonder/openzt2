use bevy::prelude::Message;
use bevy::prelude::{Component, Entity};
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldLoadFailed {
    pub operation: Option<Entity>,
    pub root: Option<Entity>,
    pub scenario: AssetId,
    pub reason: WorldLoadFailure,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldLoadFailure {
    MissingScenario,
    InvalidRecord(u32),
    MissingDependency(AssetId),
    DuplicatePersistentId(u64),
}
