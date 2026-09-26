use bevy::{prelude::Component, tasks::Task};

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailure, WorldSnapshotPersistenceOperation,
    },
    save_slot_types::{SaveSlotId, SaveSlotRecord},
};

#[derive(Component)]
pub(in crate::plugins::persistence) struct PersistenceSlotFilesystemTask {
    pub(in crate::plugins::persistence) filesystem_task:
        Task<std::io::Result<PersistenceSlotFilesystemTaskCompletion>>,
}

#[derive(Component, Debug, Clone, Copy)]
pub(in crate::plugins::persistence) struct PersistenceSlotFilesystemOperationContext {
    pub(in crate::plugins::persistence) persistence_operation: WorldSnapshotPersistenceOperation,
    pub(in crate::plugins::persistence) save_slot_identifier: SaveSlotId,
}

pub(in crate::plugins::persistence) enum PersistenceSlotFilesystemTaskCompletion {
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
