use bevy::{
    prelude::{Commands, MessageReader, MessageWriter, Or, Query, Res, With},
    tasks::IoTaskPool,
};

use super::super::{
    durable_filesystem_operations::durably_delete_file_if_present,
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    profile_types::ProfileOptions,
    save_slot_types::DeleteWorldSnapshotFromSlot,
};
use super::{
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_slot_filesystem_task_types::{
        PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
        PersistenceSlotFilesystemTaskCompletion,
    },
};

pub(in super::super) fn begin_deleting_requested_save_slot_snapshot_file(
    mut commands: Commands,
    mut delete_requests: MessageReader<DeleteWorldSnapshotFromSlot>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_options: Res<ProfileOptions>,
    active_persistence_operations: Query<
        (),
        Or<(
            With<PersistenceSlotFilesystemTask>,
            With<PendingSnapshotApplication>,
        )>,
    >,
    mut persistence_failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    let mut accepted_delete_request = false;
    for delete_request in delete_requests.read() {
        if accepted_delete_request || !active_persistence_operations.is_empty() {
            persistence_failures.write(WorldSnapshotPersistenceFailed {
                persistence_operation: WorldSnapshotPersistenceOperation::DeleteFromSlot,
                save_slot_identifier: delete_request.save_slot_identifier,
                failure_reason: WorldSnapshotPersistenceFailure::FilesystemIo,
            });
            continue;
        }
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            persistence_failures.write(WorldSnapshotPersistenceFailed {
                persistence_operation: WorldSnapshotPersistenceOperation::DeleteFromSlot,
                save_slot_identifier: delete_request.save_slot_identifier,
                failure_reason: WorldSnapshotPersistenceFailure::FilesystemIo,
            });
            continue;
        };

        let save_slot_file_path = persistence_filesystem_paths.save_slot_file_path(
            &profile_options.profile_identifier.0,
            delete_request.save_slot_identifier,
        );
        let save_slot_identifier = delete_request.save_slot_identifier;
        commands.spawn((
            PersistenceSlotFilesystemOperationContext {
                persistence_operation: WorldSnapshotPersistenceOperation::DeleteFromSlot,
                save_slot_identifier,
            },
            PersistenceSlotFilesystemTask {
                filesystem_task: IoTaskPool::get().spawn(async move {
                    Ok(match durably_delete_file_if_present(&save_slot_file_path) {
                        Ok(()) => PersistenceSlotFilesystemTaskCompletion::SnapshotFileDeleted {
                            save_slot_identifier,
                        },
                        Err(filesystem_error) => {
                            PersistenceSlotFilesystemTaskCompletion::FilesystemOperationFailed {
                                persistence_operation:
                                    WorldSnapshotPersistenceOperation::DeleteFromSlot,
                                save_slot_identifier,
                                reusable_snapshot_byte_buffer: None,
                                filesystem_error_kind: filesystem_error.kind(),
                            }
                        }
                    })
                }),
            },
        ));
        accepted_delete_request = true;
    }
}
