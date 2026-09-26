use bevy::prelude::Message;

use super::save_slot_types::SaveSlotId;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSnapshotPersistenceFailed {
    pub persistence_operation: WorldSnapshotPersistenceOperation,
    pub save_slot_identifier: SaveSlotId,
    pub failure_reason: WorldSnapshotPersistenceFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldSnapshotPersistenceOperation {
    SaveToSlot,
    LoadFromSlot,
    DeleteFromSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldSnapshotPersistenceFailure {
    SaveSlotFileMissing,
    FilesystemIo,
    InvalidSnapshotHeader,
    UnsupportedSnapshotFormatVersion,
    SnapshotProfileMismatch,
    CorruptSnapshotSection,
    UnknownAssetIdentifier,
    DuplicatePersistentEntityIdentifier,
    BrokenPersistentEntityReference,
    CapacityExceeded,
}
