use bevy::{
    prelude::{Commands, Entity, MessageWriter, Query, Res, ResMut},
    tasks::{block_on, poll_once},
};

use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogue;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogueReady;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotDeletedFromSlot;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotSavedToSlot;

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    persistence_ui_types::{SaveDialogSnapshotSaved, SaveDialogSubmission},
    snapshot_container_types::ReusableWorldSnapshotByteBuffer,
};
use super::{
    pending_snapshot_application_types::{PendingSnapshotApplication, SnapshotBytes},
    persistence_slot_filesystem_task_types::{
        PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
        PersistenceSlotFilesystemTaskCompletion,
    },
};

pub(in super::super) fn apply_completed_persistence_slot_filesystem_tasks(
    mut commands: Commands,
    mut persistence_slot_filesystem_tasks: Query<(
        Entity,
        &PersistenceSlotFilesystemOperationContext,
        Option<&SaveDialogSubmission>,
        &mut PersistenceSlotFilesystemTask,
    )>,
    profile_options: Res<ProfileOptions>,
    mut snapshot_scratch_storage: ResMut<ReusableWorldSnapshotByteBuffer>,
    mut save_catalogue: ResMut<SaveSlotCatalogue>,
    mut completed_saves: MessageWriter<WorldSnapshotSavedToSlot>,
    mut completed_dialog_saves: MessageWriter<SaveDialogSnapshotSaved>,
    mut deleted_saves: MessageWriter<WorldSnapshotDeletedFromSlot>,
    mut save_catalogue_ready_messages: MessageWriter<SaveSlotCatalogueReady>,
    mut persistence_failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    for (task_entity, operation_context, dialog, mut filesystem_task) in
        &mut persistence_slot_filesystem_tasks
    {
        let Some(task_completion) = block_on(poll_once(&mut filesystem_task.filesystem_task))
        else {
            continue;
        };
        match task_completion {
            Ok(PersistenceSlotFilesystemTaskCompletion::SnapshotFileSaved {
                record,
                reusable_snapshot_byte_buffer,
            }) => {
                let save_slot_identifier = record.save_slot_identifier;
                let saved_snapshot_byte_count = reusable_snapshot_byte_buffer.len() as u64;
                if record.profile_identifier == profile_options.profile_identifier {
                    match save_catalogue
                        .save_slot_records
                        .binary_search_by_key(&save_slot_identifier, |save_slot_record| {
                            save_slot_record.save_slot_identifier
                        }) {
                        Ok(existing_record_index) => {
                            save_catalogue.save_slot_records[existing_record_index] = record;
                        }
                        Err(insertion_index) => save_catalogue
                            .save_slot_records
                            .insert(insertion_index, record),
                    }
                    save_catalogue_ready_messages.write(SaveSlotCatalogueReady);
                }
                snapshot_scratch_storage.snapshot_container_bytes = reusable_snapshot_byte_buffer;
                snapshot_scratch_storage.clear_logical_contents();
                commands.entity(task_entity).despawn();
                completed_saves.write(WorldSnapshotSavedToSlot {
                    save_slot_identifier,
                    saved_snapshot_byte_count,
                });
                if let Some(dialog) = dialog {
                    completed_dialog_saves.write(SaveDialogSnapshotSaved {
                        operation: task_entity,
                        dialog: *dialog,
                    });
                }
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::SnapshotEncodingFailed {
                save_slot_identifier,
                reusable_snapshot_byte_buffer,
                failure_reason,
            }) => {
                snapshot_scratch_storage.snapshot_container_bytes = reusable_snapshot_byte_buffer;
                snapshot_scratch_storage.clear_logical_contents();
                commands.entity(task_entity).despawn();
                persistence_failures.write(WorldSnapshotPersistenceFailed {
                    persistence_operation: WorldSnapshotPersistenceOperation::SaveToSlot,
                    save_slot_identifier,
                    failure_reason,
                });
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::SnapshotFileLoaded {
                save_slot_identifier,
                reusable_snapshot_byte_buffer,
            }) => {
                commands
                    .entity(task_entity)
                    .remove::<PersistenceSlotFilesystemTask>()
                    .insert((
                PendingSnapshotApplication {
                    source: openzt2_interface_state::persistence::saved_game_source_types::SavedGameSource::Native(save_slot_identifier),
                    saved_world_document: None,
                },
                SnapshotBytes(reusable_snapshot_byte_buffer),
            ));
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::SnapshotFileDeleted {
                save_slot_identifier,
            }) => {
                commands.entity(task_entity).despawn();
                if let Ok(existing_record_index) = save_catalogue
                    .save_slot_records
                    .binary_search_by_key(&save_slot_identifier, |save_slot_record| {
                        save_slot_record.save_slot_identifier
                    })
                {
                    save_catalogue
                        .save_slot_records
                        .remove(existing_record_index);
                    save_catalogue_ready_messages.write(SaveSlotCatalogueReady);
                }
                deleted_saves.write(WorldSnapshotDeletedFromSlot {
                    save_slot_identifier,
                });
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::SaveCatalogueLoaded {
                save_slot_records,
            }) => {
                save_catalogue.save_slot_records = save_slot_records;
                commands.entity(task_entity).despawn();
                save_catalogue_ready_messages.write(SaveSlotCatalogueReady);
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::SaveCatalogueEntryValidationFailed {
                save_slot_identifier,
                failure_reason,
            }) => {
                commands.entity(task_entity).despawn();
                persistence_failures.write(WorldSnapshotPersistenceFailed {
                    persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                    save_slot_identifier,
                    failure_reason,
                });
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::FilesystemOperationFailed {
                persistence_operation,
                save_slot_identifier,
                reusable_snapshot_byte_buffer,
                filesystem_error_kind,
            }) => {
                commands.entity(task_entity).despawn();
                if let Some(reusable_snapshot_byte_buffer) = reusable_snapshot_byte_buffer {
                    snapshot_scratch_storage.snapshot_container_bytes =
                        reusable_snapshot_byte_buffer;
                    snapshot_scratch_storage.clear_logical_contents();
                }
                persistence_failures.write(WorldSnapshotPersistenceFailed {
                    persistence_operation,
                    save_slot_identifier,
                    failure_reason: if filesystem_error_kind == std::io::ErrorKind::NotFound {
                        WorldSnapshotPersistenceFailure::SaveSlotFileMissing
                    } else {
                        WorldSnapshotPersistenceFailure::FilesystemIo
                    },
                });
            }
            Err(filesystem_error) => {
                commands.entity(task_entity).despawn();
                let failure_reason = if filesystem_error.kind() == std::io::ErrorKind::NotFound {
                    WorldSnapshotPersistenceFailure::SaveSlotFileMissing
                } else {
                    WorldSnapshotPersistenceFailure::FilesystemIo
                };
                persistence_failures.write(WorldSnapshotPersistenceFailed {
                    persistence_operation: operation_context.persistence_operation,
                    save_slot_identifier: operation_context.save_slot_identifier,
                    failure_reason,
                });
            }
        }
    }
}
