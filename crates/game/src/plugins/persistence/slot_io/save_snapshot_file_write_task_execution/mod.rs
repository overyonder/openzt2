use std::path::PathBuf;

use bevy::{prelude::Commands, tasks::IoTaskPool};

use super::super::{
    durable_filesystem_operations::atomically_write_and_sync_file,
    persistence_failure_types::WorldSnapshotPersistenceOperation, save_slot_types::SaveSlotId,
};
use super::persistence_slot_filesystem_task_types::{
    PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
    PersistenceSlotFilesystemTaskCompletion,
};

pub(super) fn spawn_persistence_slot_snapshot_file_write_task(
    commands: &mut Commands,
    save_slot_file_path: PathBuf,
    save_slot_identifier: SaveSlotId,
    reusable_snapshot_byte_buffer: Vec<u8>,
) {
    commands.spawn((
        PersistenceSlotFilesystemOperationContext {
            persistence_operation: WorldSnapshotPersistenceOperation::SaveToSlot,
            save_slot_identifier,
        },
        PersistenceSlotFilesystemTask {
            filesystem_task: IoTaskPool::get().spawn(async move {
                Ok(
                    match atomically_write_and_sync_file(
                        &save_slot_file_path,
                        &reusable_snapshot_byte_buffer,
                    ) {
                        Ok(()) => PersistenceSlotFilesystemTaskCompletion::SnapshotFileSaved {
                            save_slot_identifier,
                            reusable_snapshot_byte_buffer,
                        },
                        Err(filesystem_error) => {
                            PersistenceSlotFilesystemTaskCompletion::FilesystemOperationFailed {
                                persistence_operation:
                                    WorldSnapshotPersistenceOperation::SaveToSlot,
                                save_slot_identifier,
                                reusable_snapshot_byte_buffer: Some(reusable_snapshot_byte_buffer),
                                filesystem_error_kind: filesystem_error.kind(),
                            }
                        }
                    },
                )
            }),
        },
    ));
}
