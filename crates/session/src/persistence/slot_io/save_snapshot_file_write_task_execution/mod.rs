use std::path::PathBuf;

use bevy::{
    ecs::system::EntityCommands,
    tasks::{AsyncComputeTaskPool, IoTaskPool},
};

use openzt2_interface_state::persistence::save_slot_types::SaveSlotRecord;

use super::super::{
    durable_filesystem_operations::atomically_write_and_sync_file,
    persistence_failure_types::{
        WorldSnapshotPersistenceFailure, WorldSnapshotPersistenceOperation,
    },
    persistence_ui_types::SaveDialogSubmission,
};
use super::persistence_slot_filesystem_task_types::{
    PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
    PersistenceSlotFilesystemTaskCompletion,
};

pub(super) fn start_persistence_slot_snapshot_file_write_task(
    mut operation: EntityCommands,
    save_slot_file_path: PathBuf,
    record: SaveSlotRecord,
    mut snapshot_bytes: Vec<u8>,
    dialog: Option<SaveDialogSubmission>,
    encode: impl FnOnce(&mut Vec<u8>) -> Result<(), WorldSnapshotPersistenceFailure> + Send + 'static,
) {
    let save_slot_identifier = record.save_slot_identifier;
    let encoding = AsyncComputeTaskPool::get().spawn(async move {
        let result = encode(&mut snapshot_bytes);
        (result, snapshot_bytes)
    });
    operation.insert((
        PersistenceSlotFilesystemOperationContext {
            persistence_operation: WorldSnapshotPersistenceOperation::SaveToSlot,
            save_slot_identifier,
        },
        PersistenceSlotFilesystemTask {
            filesystem_task: IoTaskPool::get().spawn(async move {
                let (result, reusable_snapshot_byte_buffer) = encoding.await;
                if let Err(failure_reason) = result {
                    return Ok(
                        PersistenceSlotFilesystemTaskCompletion::SnapshotEncodingFailed {
                            save_slot_identifier,
                            reusable_snapshot_byte_buffer,
                            failure_reason,
                        },
                    );
                }
                Ok(
                    match atomically_write_and_sync_file(
                        &save_slot_file_path,
                        &reusable_snapshot_byte_buffer,
                    ) {
                        Ok(()) => PersistenceSlotFilesystemTaskCompletion::SnapshotFileSaved {
                            record,
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
    if let Some(dialog) = dialog {
        operation.insert(dialog);
    }
}
