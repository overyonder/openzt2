use bevy::prelude::{Commands, MessageReader, MessageWriter, Or, Query, Res, ResMut, With};

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    profile_types::ProfileOptions,
    progression_snapshot_capture::capture_progression_snapshot_records_from_live_world,
    save_slot_types::SaveWorldSnapshotToSlot,
    snapshot_container_encoding_and_validation::initialize_empty_world_snapshot_container,
    snapshot_container_types::ReusableWorldSnapshotByteBuffer,
};
use super::{
    animal_adoption_offer_inventory_snapshot_section::validate_animal_adoption_offer_inventory_against_loaded_authored_content,
    economy_snapshot_capture::capture_economy_snapshot_records_from_live_world,
    persistence_failure_message_publication::publish_persistence_failure_for_slot_operation,
    photo_snapshot_capture::capture_sorted_photo_and_challenge_progress_snapshot_records_from_live_world,
    save_snapshot_file_write_task_execution::spawn_persistence_slot_snapshot_file_write_task,
    show_snapshot_capture::capture_sorted_show_snapshot_records_from_live_world,
    snapshot_container_section_assembly::append_captured_world_snapshot_sections_and_finalize_container,
    tank_snapshot_capture::capture_sorted_tank_snapshot_records_from_live_world,
    transport_snapshot_capture::capture_sorted_transport_snapshot_records_from_live_world,
    world_snapshot_capture::capture_world_snapshot_record_from_live_world,
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};
use super::{
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_slot_filesystem_task_types::PersistenceSlotFilesystemTask,
};

pub(in super::super) fn begin_saving_requested_live_world_snapshot_to_save_slot(
    mut commands: Commands,
    mut save_requests: MessageReader<SaveWorldSnapshotToSlot>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_options: Res<ProfileOptions>,
    active_persistence_operations: Query<
        (),
        Or<(
            With<PersistenceSlotFilesystemTask>,
            With<PendingSnapshotApplication>,
        )>,
    >,
    world_snapshot_capture_queries: WorldSnapshotCaptureQueries,
    mut snapshot_scratch_storage: ResMut<ReusableWorldSnapshotByteBuffer>,
    mut persistence_failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    let mut accepted_save_request = false;
    for save_request in save_requests.read() {
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            publish_persistence_failure_for_slot_operation(
                &mut persistence_failures,
                WorldSnapshotPersistenceOperation::SaveToSlot,
                save_request.save_slot_identifier,
                WorldSnapshotPersistenceFailure::FilesystemIo,
            );
            continue;
        };
        if accepted_save_request || !active_persistence_operations.is_empty() {
            publish_persistence_failure_for_slot_operation(
                &mut persistence_failures,
                WorldSnapshotPersistenceOperation::SaveToSlot,
                save_request.save_slot_identifier,
                WorldSnapshotPersistenceFailure::FilesystemIo,
            );
            continue;
        }
        if !world_snapshot_capture_queries
            .transport_entities_without_persistent_identifiers
            .is_empty()
            || !world_snapshot_capture_queries
                .show_platform_upgrades_without_persistent_identifiers
                .is_empty()
            || !world_snapshot_capture_queries
                .transport_circuit_members_without_definition_identifiers
                .is_empty()
        {
            publish_persistence_failure_for_slot_operation(
                &mut persistence_failures,
                WorldSnapshotPersistenceOperation::SaveToSlot,
                save_request.save_slot_identifier,
                WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference,
            );
            continue;
        }

        snapshot_scratch_storage.clear_logical_contents();
        initialize_empty_world_snapshot_container(
            &mut snapshot_scratch_storage.snapshot_container_bytes,
            profile_options.profile_identifier,
        );
        let world_snapshot_record =
            match capture_world_snapshot_record_from_live_world(&world_snapshot_capture_queries) {
                Ok(world_snapshot_record) => world_snapshot_record,
                Err(failure_reason) => {
                    publish_persistence_failure_for_slot_operation(
                        &mut persistence_failures,
                        WorldSnapshotPersistenceOperation::SaveToSlot,
                        save_request.save_slot_identifier,
                        failure_reason,
                    );
                    continue;
                }
            };
        let animal_adoption_offer_inventory = match world_snapshot_capture_queries
            .animal_adoption_offer_inventories
            .single()
        {
            Ok(inventory) => inventory,
            Err(_) => {
                snapshot_scratch_storage.clear_logical_contents();
                publish_persistence_failure_for_slot_operation(
                    &mut persistence_failures,
                    WorldSnapshotPersistenceOperation::SaveToSlot,
                    save_request.save_slot_identifier,
                    WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
                );
                continue;
            }
        };
        let animal_adoption_offer_inventory_validation = world_snapshot_capture_queries
            .active_species
            .get(&world_snapshot_capture_queries.species_assets)
            .zip(
                world_snapshot_capture_queries
                    .active_world_definitions
                    .get(&world_snapshot_capture_queries.world_definition_assets),
            )
            .and_then(|(species, world_definitions)| {
                world_definitions
                    .animal_adoption_offer_configuration()
                    .map(|configuration| (species, configuration))
            })
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)
            .and_then(|(species, configuration)| {
                validate_animal_adoption_offer_inventory_against_loaded_authored_content(
                    animal_adoption_offer_inventory,
                    species,
                    configuration,
                    *world_snapshot_capture_queries.fame,
                )
            });
        if let Err(failure_reason) = animal_adoption_offer_inventory_validation {
            snapshot_scratch_storage.clear_logical_contents();
            publish_persistence_failure_for_slot_operation(
                &mut persistence_failures,
                WorldSnapshotPersistenceOperation::SaveToSlot,
                save_request.save_slot_identifier,
                failure_reason,
            );
            continue;
        }
        let tank_snapshot_records =
            capture_sorted_tank_snapshot_records_from_live_world(&world_snapshot_capture_queries);
        let (
            show_stage_snapshot_records,
            scheduled_show_snapshot_records,
            scheduled_show_break_snapshot_records,
        ) = match capture_sorted_show_snapshot_records_from_live_world(
            &world_snapshot_capture_queries,
        ) {
            Ok(captured_show_snapshot_records) => captured_show_snapshot_records,
            Err(failure_reason) => {
                snapshot_scratch_storage.clear_logical_contents();
                publish_persistence_failure_for_slot_operation(
                    &mut persistence_failures,
                    WorldSnapshotPersistenceOperation::SaveToSlot,
                    save_request.save_slot_identifier,
                    failure_reason,
                );
                continue;
            }
        };
        let transport_snapshot_records =
            match capture_sorted_transport_snapshot_records_from_live_world(
                &world_snapshot_capture_queries,
            ) {
                Ok(transport_snapshot_records) => transport_snapshot_records,
                Err(failure_reason) => {
                    snapshot_scratch_storage.clear_logical_contents();
                    publish_persistence_failure_for_slot_operation(
                        &mut persistence_failures,
                        WorldSnapshotPersistenceOperation::SaveToSlot,
                        save_request.save_slot_identifier,
                        failure_reason,
                    );
                    continue;
                }
            };
        let economy_snapshot_records =
            capture_economy_snapshot_records_from_live_world(&world_snapshot_capture_queries);
        let (photo_snapshot_records, photo_challenge_progress_snapshot_records) =
            match capture_sorted_photo_and_challenge_progress_snapshot_records_from_live_world(
                &world_snapshot_capture_queries,
            ) {
                Ok(captured_photo_and_challenge_progress_snapshot_records) => {
                    captured_photo_and_challenge_progress_snapshot_records
                }
                Err(failure_reason) => {
                    snapshot_scratch_storage.clear_logical_contents();
                    publish_persistence_failure_for_slot_operation(
                        &mut persistence_failures,
                        WorldSnapshotPersistenceOperation::SaveToSlot,
                        save_request.save_slot_identifier,
                        failure_reason,
                    );
                    continue;
                }
            };
        let progression_snapshot_records =
            match capture_progression_snapshot_records_from_live_world(
                &world_snapshot_capture_queries.progression_snapshot_capture_queries,
            ) {
                Ok(progression_snapshot_records) => progression_snapshot_records,
                Err(failure_reason) => {
                    snapshot_scratch_storage.clear_logical_contents();
                    publish_persistence_failure_for_slot_operation(
                        &mut persistence_failures,
                        WorldSnapshotPersistenceOperation::SaveToSlot,
                        save_request.save_slot_identifier,
                        failure_reason,
                    );
                    continue;
                }
            };
        let simulation_time_snapshot_record = match world_snapshot_capture_queries
            .simulation_time_snapshot_capture_resources
            .capture_validated_simulation_time_snapshot_record()
        {
            Ok(simulation_time_snapshot_record) => simulation_time_snapshot_record,
            Err(failure_reason) => {
                snapshot_scratch_storage.clear_logical_contents();
                publish_persistence_failure_for_slot_operation(
                    &mut persistence_failures,
                    WorldSnapshotPersistenceOperation::SaveToSlot,
                    save_request.save_slot_identifier,
                    failure_reason,
                );
                continue;
            }
        };
        let snapshot_container_encoding_result =
            append_captured_world_snapshot_sections_and_finalize_container(
                &mut snapshot_scratch_storage.snapshot_container_bytes,
                &world_snapshot_record,
                animal_adoption_offer_inventory,
                &tank_snapshot_records,
                &economy_snapshot_records,
                &progression_snapshot_records,
                &simulation_time_snapshot_record,
                &show_stage_snapshot_records,
                &scheduled_show_snapshot_records,
                &scheduled_show_break_snapshot_records,
                &transport_snapshot_records,
                &photo_snapshot_records,
                &photo_challenge_progress_snapshot_records,
            );
        if let Err(failure_reason) = snapshot_container_encoding_result {
            snapshot_scratch_storage.clear_logical_contents();
            publish_persistence_failure_for_slot_operation(
                &mut persistence_failures,
                WorldSnapshotPersistenceOperation::SaveToSlot,
                save_request.save_slot_identifier,
                failure_reason,
            );
            continue;
        }

        let save_slot_file_path = persistence_filesystem_paths.save_slot_file_path(
            &profile_options.profile_identifier.0,
            save_request.save_slot_identifier,
        );
        let reusable_snapshot_byte_buffer =
            std::mem::take(&mut snapshot_scratch_storage.snapshot_container_bytes);
        spawn_persistence_slot_snapshot_file_write_task(
            &mut commands,
            save_slot_file_path,
            save_request.save_slot_identifier,
            reusable_snapshot_byte_buffer,
        );
        accepted_save_request = true;
    }
}
