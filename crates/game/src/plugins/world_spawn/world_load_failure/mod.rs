use bevy::prelude::Message;
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorldLoadFailed {
    pub(crate) scenario: AssetId,
    pub(super) reason: WorldLoadFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorldLoadFailure {
    MissingScenario,
    InvalidRecord(u32),
    MissingDependency(AssetId),
    DuplicatePersistentId(u64),
}
