use bevy::{prelude::Component, tasks::Task};

use openzt2_interface_state::persistence::save_slot_types::SaveSlotId;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotRecord;

use super::super::persistence_failure_types::{
    WorldSnapshotPersistenceFailure, WorldSnapshotPersistenceOperation,
};

#[derive(Component)]
pub(in crate::persistence) struct PersistenceSlotFilesystemTask {
    pub(in crate::persistence) filesystem_task:
        Task<std::io::Result<PersistenceSlotFilesystemTaskCompletion>>,
}

#[derive(Component, Debug, Clone, Copy)]
pub(in crate::persistence) struct PersistenceSlotFilesystemOperationContext {
    pub(in crate::persistence) persistence_operation: WorldSnapshotPersistenceOperation,
    pub(in crate::persistence) save_slot_identifier: SaveSlotId,
}

pub(in crate::persistence) enum PersistenceSlotFilesystemTaskCompletion {
    SnapshotFileSaved {
        save_slot_identifier: SaveSlotId,
        reusable_snapshot_byte_buffer: Vec<u8>,
    },
    SnapshotFileLoaded {
        save_slot_identifier: SaveSlotId,
        reusable_snapshot_byte_buffer: Vec<u8>,
    },
    SnapshotFileDeleted {
        save_slot_identifier: SaveSlotId,
    },
    SaveCatalogueLoaded {
        save_slot_records: Vec<SaveSlotRecord>,
    },
    SaveCatalogueEntryValidationFailed {
        save_slot_identifier: SaveSlotId,
        failure_reason: WorldSnapshotPersistenceFailure,
    },
    FilesystemOperationFailed {
        persistence_operation: WorldSnapshotPersistenceOperation,
        save_slot_identifier: SaveSlotId,
        reusable_snapshot_byte_buffer: Option<Vec<u8>>,
        filesystem_error_kind: std::io::ErrorKind,
    },
}
