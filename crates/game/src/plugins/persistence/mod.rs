//! Native OpenZT2 profiles and deterministic ECS snapshot persistence.

mod durable_filesystem_operations;
#[cfg(test)]
mod durable_filesystem_operations_tests;
mod generated_photo_file_persistence;
pub mod generated_photo_persistence_types;
mod initial_profile_activation;
mod overview_map_html_and_jpeg_export;
pub mod persistence_failure_types;
pub mod persistence_filesystem_paths;
mod persistence_ui_action_routing;
mod persistence_ui_operation_presentation;
mod persistence_ui_types;
mod photo_album_html_and_bmp_export;
mod profile_filesystem_task_completion_application;
mod profile_identifier_generation;
mod profile_index_encoding;
#[cfg(test)]
mod profile_index_encoding_tests;
mod profile_persistence_types;
mod profile_request_execution;
pub mod profile_types;
mod progression_snapshot_application;
mod progression_snapshot_capture;
mod progression_snapshot_section_encoding;
#[cfg(test)]
mod progression_snapshot_section_encoding_tests;
mod progression_snapshot_types;
mod progression_snapshot_validation;
mod rgba8_bmp_encoding;
mod save_slot_catalogue_ui_presentation;
pub mod save_slot_types;
mod screenshot_export;
mod simulation_time_snapshot_section;
mod slot_io;
mod snapshot_container_encoding_and_validation;
#[cfg(test)]
mod snapshot_container_tests;
mod snapshot_container_types;

use crate::{
    application_lifecycle::GamePhase, application_schedule::GameSet,
    plugins::world_spawn::persistent_id_types::PersistentIdAllocator,
};
use bevy::prelude::*;

use generated_photo_persistence_types::{
    ChallengePhotoJpegCopyPersisted, ChallengePhotoJpegCopyPersistenceFailed,
    DeleteGeneratedPhotoImageAndChallengeCopies, GeneratedPhotoImageAndChallengeCopiesDeleted,
    GeneratedPhotoImageAndChallengeCopiesDeletionFailed, PersistChallengePhotoJpegCopy,
};

use persistence_failure_types::WorldSnapshotPersistenceFailed;
use profile_types::{
    CreateProfile, DeleteProfile, LoadProfileIndex, ProfileCreated, ProfileDeleted, ProfileIndex,
    ProfileIndexReady, ProfileOperationFailed, ProfileOptions, ProfileSelected, SelectProfile,
};
use save_slot_types::{
    DeleteWorldSnapshotFromSlot, LoadSaveSlotCatalogue, LoadWorldSnapshotFromSlot,
    SaveSlotCatalogue, SaveSlotCatalogueReady, SaveWorldSnapshotToSlot,
    WorldSnapshotDeletedFromSlot, WorldSnapshotLoadedFromSlot, WorldSnapshotSavedToSlot,
};
use snapshot_container_types::ReusableWorldSnapshotByteBuffer;

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReusableWorldSnapshotByteBuffer>()
            .init_resource::<ProfileIndex>()
            .init_resource::<ProfileOptions>()
            .init_resource::<SaveSlotCatalogue>()
            .init_resource::<crate::plugins::progression::profile_challenge_types::ProfileChallengeCompletionCounts>()
            .add_message::<SaveWorldSnapshotToSlot>()
            .add_message::<LoadWorldSnapshotFromSlot>()
            .add_message::<DeleteWorldSnapshotFromSlot>()
            .add_message::<WorldSnapshotSavedToSlot>()
            .add_message::<WorldSnapshotLoadedFromSlot>()
            .add_message::<WorldSnapshotDeletedFromSlot>()
            .add_message::<LoadSaveSlotCatalogue>()
            .add_message::<SaveSlotCatalogueReady>()
            .add_message::<DeleteGeneratedPhotoImageAndChallengeCopies>()
            .add_message::<GeneratedPhotoImageAndChallengeCopiesDeleted>()
            .add_message::<GeneratedPhotoImageAndChallengeCopiesDeletionFailed>()
            .add_message::<PersistChallengePhotoJpegCopy>()
            .add_message::<ChallengePhotoJpegCopyPersisted>()
            .add_message::<ChallengePhotoJpegCopyPersistenceFailed>()
            .add_message::<crate::plugins::photos::photo_album_types::ExportPhotoAlbum>()
            .add_message::<photo_album_html_and_bmp_export::PhotoAlbumHtmlAndBmpExported>()
            .add_message::<photo_album_html_and_bmp_export::PhotoAlbumHtmlAndBmpExportFailed>()
            .add_message::<overview_map_html_and_jpeg_export::OverviewMapHtmlAndJpegExported>()
            .add_message::<overview_map_html_and_jpeg_export::OverviewMapHtmlAndJpegExportFailed>()
            .add_message::<screenshot_export::FullFrameScreenshotBmpExported>()
            .add_message::<screenshot_export::FullFrameScreenshotBmpExportFailed>()
            .add_message::<WorldSnapshotPersistenceFailed>()
            .add_message::<LoadProfileIndex>()
            .add_message::<CreateProfile>()
            .add_message::<SelectProfile>()
            .add_message::<DeleteProfile>()
            .add_message::<ProfileIndexReady>()
            .add_message::<ProfileCreated>()
            .add_message::<ProfileSelected>()
            .add_message::<ProfileDeleted>()
            .add_message::<ProfileOperationFailed>()
            .add_systems(
                Startup,
                initial_profile_activation::request_initial_profile_index_from_persistence_startup,
            )
            .add_systems(
                Update,
                (
                    profile_request_execution::begin_loading_profile_index,
                    profile_request_execution::begin_creating_requested_profiles,
                    profile_request_execution::begin_selecting_requested_profiles,
                    profile_request_execution::begin_deleting_requested_profiles,
                    profile_filesystem_task_completion_application::
                        apply_completed_profile_filesystem_tasks_to_canonical_profile_state,
                    initial_profile_activation::
                        select_persisted_profile_or_create_localized_first_profile,
                    profile_request_execution::begin_persisting_changed_active_profile_options,
                    persistence_ui_action_routing::route_authored_persistence_ui_actions_to_domain_requests,
                    save_slot_catalogue_ui_presentation::load_save_slot_from_submitted_row,
                    save_slot_catalogue_ui_presentation::request_save_slot_catalogue_list_row_count_updates,
                    save_slot_catalogue_ui_presentation::project_save_slot_catalogue_records_onto_saved_game_list_rows,
                    persistence_ui_operation_presentation::project_outstanding_world_snapshot_operations_onto_ui_wait_cursor,
                    persistence_ui_operation_presentation::continue_to_load_slot_catalogue_after_world_snapshot_save,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    photo_album_html_and_bmp_export::begin_photo_album_html_and_bmp_exports,
                    photo_album_html_and_bmp_export::complete_photo_album_html_and_bmp_exports,
                    overview_map_html_and_jpeg_export::begin_overview_map_html_and_jpeg_exports,
                    overview_map_html_and_jpeg_export::complete_overview_map_html_and_jpeg_exports,
                    screenshot_export::begin_full_frame_screenshot_bmp_exports,
                    screenshot_export::complete_full_frame_screenshot_bmp_exports,
                    generated_photo_file_persistence::begin_challenge_photo_writes,
                    generated_photo_file_persistence::complete_challenge_photo_writes,
                    generated_photo_file_persistence::begin_generated_image_deletes,
                    generated_photo_file_persistence::complete_generated_image_deletes,
                )
                    .chain()
                    .after(persistence_ui_operation_presentation::continue_to_load_slot_catalogue_after_world_snapshot_save),
            )
            .add_systems(
                Update,
                (
                    slot_io::save_snapshot_request_execution::
                        begin_saving_requested_live_world_snapshot_to_save_slot
                        .run_if(in_state(GamePhase::InGame)),
                    slot_io::load_snapshot_file_request_execution::
                        begin_loading_requested_world_snapshot_from_save_slot,
                    slot_io::delete_snapshot_file_request_execution::
                        begin_deleting_requested_save_slot_snapshot_file,
                    slot_io::save_slot_catalogue_load_execution::
                        begin_loading_save_slot_catalogue_for_active_profile,
                    slot_io::filesystem_task_completion_application::
                        apply_completed_persistence_slot_filesystem_tasks,
                )
                    .chain()
                    .after(persistence_ui_action_routing::route_authored_persistence_ui_actions_to_domain_requests)
                    .after(
                        photo_album_html_and_bmp_export::complete_photo_album_html_and_bmp_exports,
                    ),
            )
            .add_systems(
                Update,
                slot_io::loaded_snapshot_application_execution::
                    coordinate_pending_snapshot_validation_baseline_world_loading_and_application
                    .after(GameSet::Intent)
                    .after(
                        slot_io::filesystem_task_completion_application::
                            apply_completed_persistence_slot_filesystem_tasks,
                    )
                    .run_if(resource_exists::<PersistentIdAllocator>),
            );
    }
}
