use super::save_slot_types::SaveSlotId;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedGameSource {
    Native(SaveSlotId),
    Original(AssetId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalSavedGameRecord {
    pub source_identifier: AssetId,
    pub world_display_name: String,
    pub profile_identifier: AssetId,
    pub profile_display_name: String,
    pub last_saved_unix_timestamp_milliseconds: u64,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadOriginalSavedGame {
    pub source_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalSavedGameLoaded {
    pub source_identifier: AssetId,
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct OriginalSavedGameLoadFailed {
    pub source_identifier: AssetId,
    pub reason: String,
}
