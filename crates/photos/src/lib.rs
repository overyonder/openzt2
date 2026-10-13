//! Player photo capture as ordinary ECS facts and Bevy render-target screenshots.

mod album_actions;
mod album_operations;
mod album_pointer_drag;
mod album_presentation;
mod photo_album_storage;
mod photo_album_types;
mod photo_capture;
mod photo_capture_types;
mod photo_challenge_hydration;
mod photo_challenge_lua_scoring;
mod photo_challenge_lua_scoring_types;
mod photo_challenge_lua_virtual_machine_invalidation;
mod photo_challenge_scoring;
mod photo_challenge_types;
mod photo_evidence_operations;
mod photo_mode_presentation_lifecycle;
mod photo_subject_evidence_collection;
mod screenshot_capture;

use openzt2_interface_state::photos::photo_capture_types::CapturePhotoRequest;
use openzt2_interface_state::photos::photo_capture_types::CaptureScreenshotRequest;
use openzt2_interface_state::photos::photo_capture_types::PhotoReadback;
use openzt2_interface_state::photos::photo_capture_types::ScreenshotCaptured;

use photo_album_storage::{
    delete_requested_photos_and_release_unpersisted_images,
    initialize_default_photo_album_and_camera_roll_for_loaded_worlds,
};
use photo_album_types::DeletePhotoRequest;
use photo_capture::begin_requested_album_photo_captures;
use photo_capture_types::{
    PhotoCaptureFailed, PhotoCaptured, PhotoChallengeCompleted, ScreenshotCaptureFailed,
};
use photo_challenge_hydration::hydrate_photo_challenges_and_sets_from_active_scenario_catalogue;
use photo_challenge_scoring::{
    score_pending_photos_against_incomplete_lua_challenges,
    update_photo_challenge_set_progress_from_completed_challenges,
};
use photo_subject_evidence_collection::collect_visible_photo_subjects_and_semantic_evidence_for_pending_captures;
use screenshot_capture::begin_requested_camera_screenshot_captures;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::ui::UiSet;
use openzt2_world_state::application_schedule::GameSet;

pub struct PhotosPlugin;

impl Plugin for PhotosPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<
            photo_challenge_lua_scoring_types::PhotoChallengeLuaScoringVirtualMachineContexts,
        >()
            .add_message::<CapturePhotoRequest>()
            .add_message::<CaptureScreenshotRequest>()
            .add_message::<ScreenshotCaptured>()
            .add_message::<ScreenshotCaptureFailed>()
            .init_resource::<PhotoReadback>()
            .add_message::<PhotoCaptured>()
            .add_message::<PhotoChallengeCompleted>()
            .add_message::<PhotoCaptureFailed>()
            .add_message::<DeletePhotoRequest>()
            .add_systems(
                Update,
                (
                    album_actions::select_photo_storage_entity_from_authored_row_activation,
                    album_pointer_drag::route_photo_exposure_pointer_drag_to_authored_album_controls,
                    album_actions::route_photo_ui_actions,
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    begin_requested_album_photo_captures
                        .after(initialize_default_photo_album_and_camera_roll_for_loaded_worlds),
                    begin_requested_camera_screenshot_captures,
                    delete_requested_photos_and_release_unpersisted_images,
                    photo_mode_presentation_lifecycle::
                        show_authored_photo_mode_document_for_entered_photo_mode,
                    photo_mode_presentation_lifecycle::
                        hide_authored_photo_mode_document_after_photo_mode_exit,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                album_presentation::set_photo_album_control_interaction_availability_from_album_state
                    .in_set(UiSet::DomainProjection)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    collect_visible_photo_subjects_and_semantic_evidence_for_pending_captures,
                    album_presentation::request_authored_photo_camera_roll_and_album_choice_row_counts,
                    album_presentation::present_photo_camera_roll_and_album_choices_in_authored_rows,
                    album_presentation::present_active_photo_album_and_camera_roll_image_slots,
                    album_presentation::present_active_photo_album_photo_count,
                    album_presentation::present_photo_album_and_photo_mode_camera_roll_text,
                    album_presentation::present_default_photo_captions_from_canonical_subjects,
                    photo_mode_presentation_lifecycle::
                        show_newly_captured_photo_in_authored_photo_mode_preview,
                    photo_mode_presentation_lifecycle::
                        initialize_authored_photo_mode_transient_surface_visibility_after_document_projection,
                    photo_mode_presentation_lifecycle::
                        advance_and_retire_authored_last_captured_photo_preview,
                )
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            );
        app.add_systems(
            Update,
            photo_challenge_lua_virtual_machine_invalidation::
                invalidate_photo_challenge_scoring_virtual_machines_affected_by_changed_assets,
        );
        app.add_systems(
            Update,
            (
                initialize_default_photo_album_and_camera_roll_for_loaded_worlds
                    .in_set(openzt2_interface_state::photos::photo_album_types::DefaultPhotoStorageInitializationSet),
                hydrate_photo_challenges_and_sets_from_active_scenario_catalogue,
                photo_album_storage::initialize_localized_default_photo_album_names
                    .after(openzt2_interface_state::photos::photo_album_types::DefaultPhotoStorageInitializationSet),
            )
                .in_set(GameSet::Intent)
                .after(openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompletionSet)
                .run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame)))
                .run_if(
                    resource_exists::<
                        openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator,
                    >,
                ),
        );
        app.add_systems(
            Update,
            (
                score_pending_photos_against_incomplete_lua_challenges,
                update_photo_challenge_set_progress_from_completed_challenges,
            )
                .chain()
                .after(hydrate_photo_challenges_and_sets_from_active_scenario_catalogue)
                .in_set(GameSet::Intent)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}

#[cfg(test)]
mod tests;
