use bevy::{
    prelude::{Commands, MessageReader, MessageWriter, Or, Query, Res, ResMut, With},
    tasks::IoTaskPool,
};

use super::super::{
    durable_filesystem_operations::read_file_into_reused_buffer_with_maximum_byte_count,
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    profile_types::ProfileOptions,
    save_slot_types::LoadWorldSnapshotFromSlot,
    snapshot_container_encoding_and_validation::MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT,
    snapshot_container_types::ReusableWorldSnapshotByteBuffer,
};
use super::{
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_slot_filesystem_task_types::{
        PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
        PersistenceSlotFilesystemTaskCompletion,
    },
};

pub(in super::super) fn begin_loading_requested_world_snapshot_from_save_slot(
    mut commands: Commands,
    mut load_requests: MessageReader<LoadWorldSnapshotFromSlot>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_options: Res<ProfileOptions>,
    active_persistence_operations: Query<
        (),
        Or<(
            With<PersistenceSlotFilesystemTask>,
            With<PendingSnapshotApplication>,
        )>,
    >,
    mut snapshot_scratch_storage: ResMut<ReusableWorldSnapshotByteBuffer>,
    mut persistence_failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    let mut accepted_load_request = false;
    for load_request in load_requests.read() {
        if accepted_load_request || !active_persistence_operations.is_empty() {
            persistence_failures.write(WorldSnapshotPersistenceFailed {
                persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                save_slot_identifier: load_request.save_slot_identifier,
                failure_reason: WorldSnapshotPersistenceFailure::FilesystemIo,
            });
            continue;
        }
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            persistence_failures.write(WorldSnapshotPersistenceFailed {
                persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                save_slot_identifier: load_request.save_slot_identifier,
                failure_reason: WorldSnapshotPersistenceFailure::FilesystemIo,
            });
            continue;
        };

        snapshot_scratch_storage.clear_logical_contents();
        let mut reusable_snapshot_byte_buffer =
            std::mem::take(&mut snapshot_scratch_storage.snapshot_container_bytes);
        let save_slot_file_path = persistence_filesystem_paths.save_slot_file_path(
            &profile_options.profile_identifier.0,
            load_request.save_slot_identifier,
        );
        let save_slot_identifier = load_request.save_slot_identifier;
        commands.spawn((
            PersistenceSlotFilesystemOperationContext {
                persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                save_slot_identifier,
            },
            PersistenceSlotFilesystemTask {
                filesystem_task: IoTaskPool::get().spawn(async move {
                    Ok(
                        match read_file_into_reused_buffer_with_maximum_byte_count(
                            &save_slot_file_path,
                            MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT,
                            &mut reusable_snapshot_byte_buffer,
                        ) {
                            Ok(()) => PersistenceSlotFilesystemTaskCompletion::SnapshotFileLoaded {
                                save_slot_identifier,
                                reusable_snapshot_byte_buffer,
                            },
                            Err(filesystem_error) => {
                                PersistenceSlotFilesystemTaskCompletion::FilesystemOperationFailed {
                                    persistence_operation:
                                        WorldSnapshotPersistenceOperation::LoadFromSlot,
                                    save_slot_identifier,
                                    reusable_snapshot_byte_buffer: Some(
                                        reusable_snapshot_byte_buffer,
                                    ),
                                    filesystem_error_kind: filesystem_error.kind(),
                                }
                            }
                        },
                    )
                }),
            },
        ));
        accepted_load_request = true;
    }
}
