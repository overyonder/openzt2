//! Native OpenZT2 profiles and deterministic ECS snapshot persistence.

mod durable_filesystem_operations;
#[cfg(test)]
mod durable_filesystem_operations_tests;
mod generated_photo_file_persistence;
mod initial_profile_activation;
mod overview_map_html_and_jpeg_export;
pub mod persistence_failure_types;
pub mod persistence_filesystem_paths;
mod persistence_ui_action_routing;
mod persistence_ui_operation_presentation;
pub mod persistence_ui_types;
mod photo_album_html_and_bmp_export;
mod profile_filesystem_task_completion_application;
mod profile_identifier_generation;
mod profile_index_encoding;
#[cfg(test)]
mod profile_index_encoding_tests;
mod profile_persistence_types;
mod profile_request_execution;
mod profile_scenario_variable_encoding;
mod progression_snapshot_application;
mod progression_snapshot_capture;
mod progression_snapshot_section_encoding;
#[cfg(test)]
mod progression_snapshot_section_encoding_tests;
mod progression_snapshot_types;
mod progression_snapshot_validation;
mod rgba8_bmp_encoding;
mod save_slot_catalogue_ui_presentation;
mod screenshot_export;
mod simulation_time_snapshot_section;
pub(crate) mod slot_io;
mod snapshot_container_encoding_and_validation;
#[cfg(test)]
mod snapshot_container_tests;
mod snapshot_container_types;
#[cfg(test)]
mod world_load_failure_recovery_tests;
#[cfg(test)]
mod world_photo_baseline_tests;

use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::GameSet;

use openzt2_interface_state::persistence::generated_photo_persistence_types::ChallengePhotoJpegCopyPersisted;
use openzt2_interface_state::persistence::generated_photo_persistence_types::ChallengePhotoJpegCopyPersistenceFailed;
use openzt2_interface_state::persistence::generated_photo_persistence_types::DeleteGeneratedPhotoImageAndChallengeCopies;
use openzt2_interface_state::persistence::generated_photo_persistence_types::GeneratedPhotoImageAndChallengeCopiesDeleted;
use openzt2_interface_state::persistence::generated_photo_persistence_types::GeneratedPhotoImageAndChallengeCopiesDeletionFailed;
use openzt2_interface_state::persistence::generated_photo_persistence_types::PersistChallengePhotoJpegCopy;

use openzt2_interface_state::persistence::profile_types::CreateProfile;
use openzt2_interface_state::persistence::profile_types::DeleteProfile;
use openzt2_interface_state::persistence::profile_types::LoadProfileIndex;
use openzt2_interface_state::persistence::profile_types::ProfileCreated;
use openzt2_interface_state::persistence::profile_types::ProfileDeleted;
use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_interface_state::persistence::profile_types::ProfileIndexReady;
use openzt2_interface_state::persistence::profile_types::ProfileOperationFailed;
use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::profile_types::ProfileSelected;
use openzt2_interface_state::persistence::profile_types::SelectProfile;
use openzt2_interface_state::persistence::save_slot_types::DeleteWorldSnapshotFromSlot;
use openzt2_interface_state::persistence::save_slot_types::LoadSaveSlotCatalogue;
use openzt2_interface_state::persistence::save_slot_types::LoadWorldSnapshotFromSlot;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogue;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogueReady;
use openzt2_interface_state::persistence::save_slot_types::SaveWorldSnapshotToSlot;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotDeletedFromSlot;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotSavedToSlot;
use persistence_failure_types::WorldSnapshotPersistenceFailed;
use snapshot_container_types::ReusableWorldSnapshotByteBuffer;

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            persistence_ui_action_routing::route_authored_save_dialog_opening_requests
                .in_set(openzt2_interface_state::ui::UiSet::Routing),
        );
        app.init_resource::<ReusableWorldSnapshotByteBuffer>()
            .init_resource::<ProfileIndex>()
            .init_resource::<ProfileOptions>()
            .init_resource::<openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables>()
            .add_message::<openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables>()
            .init_resource::<SaveSlotCatalogue>()
            .init_resource::<openzt2_zoo_state::progression::profile_challenge_types::ProfileChallengeCompletionCounts>()
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
            .add_message::<openzt2_interface_state::photos::photo_album_types::ExportPhotoAlbum>()
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
                    profile_request_execution::begin_selecting_requested_profiles.after(GameSet::Intent),
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
                    persistence_ui_operation_presentation::continue_to_load_slot_catalogue_after_world_snapshot_save
                        .after(openzt2_interface_state::ui::UiSet::Projection),
                    persistence_ui_operation_presentation::close_save_dialogs_after_world_snapshot_save,
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
                        .after(openzt2_interface_state::ui::UiSet::DomainProjection)
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
                    ),
            );
    }
}
