use bevy::{
    prelude::{Commands, MessageReader, MessageWriter, Or, Query, Res, With},
    tasks::IoTaskPool,
};

use super::super::{
    durable_filesystem_operations::read_file_with_maximum_byte_count,
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    profile_types::{ProfileOptions, ProfileSelected},
    save_slot_types::{LoadSaveSlotCatalogue, SaveSlotId, SaveSlotRecord},
    snapshot_container_encoding_and_validation::{
        validate_world_snapshot_container_and_section_directory,
        MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT,
    },
    snapshot_container_types::WorldSnapshotSectionKind,
};
use super::{
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_failure_message_publication::publish_persistence_failure_for_slot_operation,
    persistence_slot_filesystem_task_types::{
        PersistenceSlotFilesystemOperationContext, PersistenceSlotFilesystemTask,
        PersistenceSlotFilesystemTaskCompletion,
    },
    world_snapshot_section_encoding::decode_and_validate_world_snapshot_section,
};

pub(in super::super) fn begin_loading_save_slot_catalogue_for_active_profile(
    mut commands: Commands,
    mut catalogue_load_requests: MessageReader<LoadSaveSlotCatalogue>,
    mut selected_profiles: MessageReader<ProfileSelected>,
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
    let catalogue_load_requested =
        catalogue_load_requests.read().next().is_some() | selected_profiles.read().next().is_some();
    if !catalogue_load_requested || !active_persistence_operations.is_empty() {
        return;
    }
    let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref().cloned()
    else {
        publish_persistence_failure_for_slot_operation(
            &mut persistence_failures,
            WorldSnapshotPersistenceOperation::LoadFromSlot,
            SaveSlotId(0),
            WorldSnapshotPersistenceFailure::FilesystemIo,
        );
        return;
    };

    let active_profile_identifier = profile_options.profile_identifier;
    commands.spawn((
        PersistenceSlotFilesystemOperationContext {
            persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
            save_slot_identifier: SaveSlotId(0),
        },
        PersistenceSlotFilesystemTask {
            filesystem_task: IoTaskPool::get().spawn(async move {
                let save_slot_file_paths = match persistence_filesystem_paths
                    .list_existing_save_slot_file_paths(&active_profile_identifier.0)
                {
                    Ok(save_slot_file_paths) => save_slot_file_paths,
                    Err(filesystem_error) => {
                        return Ok(
                            PersistenceSlotFilesystemTaskCompletion::FilesystemOperationFailed {
                                persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                                save_slot_identifier: SaveSlotId(0),
                                reusable_snapshot_byte_buffer: None,
                                filesystem_error_kind: filesystem_error.kind(),
                            },
                        );
                    }
                };

                let mut save_slot_records = Vec::with_capacity(save_slot_file_paths.len());
                for (save_slot_identifier, save_slot_file_path) in save_slot_file_paths {
                    let snapshot_bytes = match read_file_with_maximum_byte_count(
                        &save_slot_file_path,
                        MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT,
                    ) {
                        Ok(snapshot_bytes) => snapshot_bytes,
                        Err(filesystem_error) => {
                            return Ok(
                                PersistenceSlotFilesystemTaskCompletion::FilesystemOperationFailed {
                                    persistence_operation: WorldSnapshotPersistenceOperation::LoadFromSlot,
                                    save_slot_identifier,
                                    reusable_snapshot_byte_buffer: None,
                                    filesystem_error_kind: filesystem_error.kind(),
                                },
                            );
                        }
                    };
                    let validated_snapshot =
                        match validate_world_snapshot_container_and_section_directory(
                            &snapshot_bytes,
                            active_profile_identifier,
                        ) {
                            Ok(validated_snapshot) => validated_snapshot,
                            Err(failure_reason) => {
                                return Ok(
                                    PersistenceSlotFilesystemTaskCompletion::SaveCatalogueEntryValidationFailed {
                                        save_slot_identifier,
                                        failure_reason,
                                    },
                                );
                            }
                        };
                    let world_snapshot = match decode_and_validate_world_snapshot_section(
                        &snapshot_bytes,
                        validated_snapshot.section_directory_entry(WorldSnapshotSectionKind::World),
                    ) {
                        Ok(world_snapshot) => world_snapshot,
                        Err(failure_reason) => {
                            return Ok(
                                PersistenceSlotFilesystemTaskCompletion::SaveCatalogueEntryValidationFailed {
                                    save_slot_identifier,
                                    failure_reason,
                                },
                            );
                        }
                    };
                    save_slot_records.push(SaveSlotRecord {
                        save_slot_identifier,
                        world_display_name: world_snapshot.display_name,
                        last_saved_unix_timestamp_milliseconds: world_snapshot
                            .last_saved_unix_timestamp_milliseconds,
                        scenario_definition_identifier: world_snapshot
                            .scenario_definition_identifier,
                        map_definition_identifier: world_snapshot
                            .map_definition_identifier,
                        profile_identifier: active_profile_identifier,
                    });
                }
                Ok(PersistenceSlotFilesystemTaskCompletion::SaveCatalogueLoaded {
                    save_slot_records,
                })
            }),
        },
    ));
}
