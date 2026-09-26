use bevy::{
    prelude::{Commands, Entity, MessageWriter, Query, Res, ResMut},
    tasks::{block_on, poll_once},
};

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    profile_types::ProfileOptions,
    save_slot_types::{
        SaveSlotCatalogue, SaveSlotCatalogueReady, SaveSlotRecord, WorldSnapshotDeletedFromSlot,
        WorldSnapshotSavedToSlot,
    },
    snapshot_container_encoding_and_validation::validate_world_snapshot_container_and_section_directory,
    snapshot_container_types::{ReusableWorldSnapshotByteBuffer, WorldSnapshotSectionKind},
};
use super::{
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_slot_filesystem_task_types::{
        PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
        PersistenceSlotFilesystemTaskCompletion,
    },
    world_snapshot_section_encoding::decode_and_validate_world_snapshot_section,
};

pub(in super::super) fn apply_completed_persistence_slot_filesystem_tasks(
    mut commands: Commands,
    mut persistence_slot_filesystem_tasks: Query<(
        Entity,
        &PersistenceSlotFilesystemOperationContext,
        &mut PersistenceSlotFilesystemTask,
    )>,
    profile_options: Res<ProfileOptions>,
    mut snapshot_scratch_storage: ResMut<ReusableWorldSnapshotByteBuffer>,
    mut save_catalogue: ResMut<SaveSlotCatalogue>,
    mut completed_saves: MessageWriter<WorldSnapshotSavedToSlot>,
    mut deleted_saves: MessageWriter<WorldSnapshotDeletedFromSlot>,
    mut save_catalogue_ready_messages: MessageWriter<SaveSlotCatalogueReady>,
    mut persistence_failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    for (task_entity, operation_context, mut filesystem_task) in
        &mut persistence_slot_filesystem_tasks
    {
        let Some(task_completion) = block_on(poll_once(&mut filesystem_task.filesystem_task))
        else {
            continue;
        };
        match task_completion {
            Ok(PersistenceSlotFilesystemTaskCompletion::SnapshotFileSaved {
                save_slot_identifier,
                reusable_snapshot_byte_buffer,
            }) => {
                let saved_snapshot_byte_count = reusable_snapshot_byte_buffer.len() as u64;
                if let Ok(validated_snapshot) =
                    validate_world_snapshot_container_and_section_directory(
                        &reusable_snapshot_byte_buffer,
                        profile_options.profile_identifier,
                    )
                {
                    if let Ok(world_snapshot) = decode_and_validate_world_snapshot_section(
                        &reusable_snapshot_byte_buffer,
                        validated_snapshot.section_directory_entry(WorldSnapshotSectionKind::World),
                    ) {
                        let save_slot_record = SaveSlotRecord {
                            save_slot_identifier,
                            world_display_name: world_snapshot.display_name,
                            last_saved_unix_timestamp_milliseconds: world_snapshot
                                .last_saved_unix_timestamp_milliseconds,
                            scenario_definition_identifier: world_snapshot
                                .scenario_definition_identifier,
                            map_definition_identifier: world_snapshot.map_definition_identifier,
                            profile_identifier: profile_options.profile_identifier,
                        };
                        match save_catalogue
                            .save_slot_records
                            .binary_search_by_key(&save_slot_identifier, |save_slot_record| {
                                save_slot_record.save_slot_identifier
                            }) {
                            Ok(existing_record_index) => {
                                save_catalogue.save_slot_records[existing_record_index] =
                                    save_slot_record;
                            }
                            Err(insertion_index) => save_catalogue
                                .save_slot_records
                                .insert(insertion_index, save_slot_record),
                        }
                        save_catalogue_ready_messages.write(SaveSlotCatalogueReady);
                    }
                }
                snapshot_scratch_storage.snapshot_container_bytes = reusable_snapshot_byte_buffer;
                snapshot_scratch_storage.clear_logical_contents();
                commands.entity(task_entity).despawn();
                completed_saves.write(WorldSnapshotSavedToSlot {
                    save_slot_identifier,
                    saved_snapshot_byte_count,
                });
            }
            Ok(PersistenceSlotFilesystemTaskCompletion::SnapshotFileLoaded {
                save_slot_identifier,
                reusable_snapshot_byte_buffer,
            }) => match validate_world_snapshot_container_and_section_directory(
                &reusable_snapshot_byte_buffer,
                profile_options.profile_identifier,
            ) {
                Ok(_) => {
                    snapshot_scratch_storage.snapshot_container_bytes =
                        reusable_snapshot_byte_buffer;
                    snapshot_scratch_storage.contains_validated_snapshot_container = false;
                    commands
                        .entity(task_entity)
                        .remove::<PersistenceSlotFilesystemTask>()
                        .insert(PendingSnapshotApplication {
                            save_slot_identifier,
                            scenario_definition_identifier: None,
                            session_mode: None,
                            baseline_world_load_requested: false,
                            baseline_world_load_completed: false,
                        });
                }
                Err(failure_reason) => {
                    snapshot_scratch_storage.snapshot_container_bytes =
                        reusable_snapshot_byte_buffer;
                    snapshot_scratch_storage.clear_logical_contents();
                    commands.entity(task_entity).despawn();
                    persistence_failures.write(WorldSnapshotPersistenceFailed {
                        persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                        save_slot_identifier,
                        failure_reason,
                    });
                }
            },
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
