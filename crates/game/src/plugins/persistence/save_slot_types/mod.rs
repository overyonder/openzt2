use bevy::prelude::{Message, Resource};
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SaveSlotId(pub u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveWorldSnapshotToSlot {
    pub save_slot_identifier: SaveSlotId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadWorldSnapshotFromSlot {
    pub save_slot_identifier: SaveSlotId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteWorldSnapshotFromSlot {
    pub save_slot_identifier: SaveSlotId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSnapshotSavedToSlot {
    pub save_slot_identifier: SaveSlotId,
    pub saved_snapshot_byte_count: u64,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSnapshotLoadedFromSlot {
    pub save_slot_identifier: SaveSlotId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSnapshotDeletedFromSlot {
    pub save_slot_identifier: SaveSlotId,
}

/// Durable catalogue facts read from one validated native snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSlotRecord {
    pub save_slot_identifier: SaveSlotId,
    pub world_display_name: String,
    pub last_saved_unix_timestamp_milliseconds: u64,
    pub scenario_definition_identifier: AssetId,
    pub map_definition_identifier: AssetId,
    pub profile_identifier: AssetId,
}

/// Available saves for the selected profile.
#[derive(Resource, Debug, Default)]
pub struct SaveSlotCatalogue {
    pub save_slot_records: Vec<SaveSlotRecord>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSaveSlotCatalogue;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveSlotCatalogueReady;
