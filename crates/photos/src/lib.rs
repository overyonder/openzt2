//! Player photo capture as ordinary ECS facts and Bevy render-target screenshots.

mod album_actions;
mod album_operations;
mod album_pointer_drag;
mod album_presentation;
mod photo_album_storage;
mod photo_album_types;
mod photo_caption;
pub use photo_caption::PhotoCaption;
mod photo_capture;
mod photo_capture_types;
mod photo_challenge_goal_presentation;
mod photo_challenge_hydration;
mod photo_challenge_lua_scoring;
mod photo_challenge_lua_scoring_types;
mod photo_challenge_lua_virtual_machine_invalidation;
mod photo_challenge_scoring;
mod photo_configuration;
mod photo_mode_presentation_lifecycle;
mod photo_subject_evidence_collection;
mod screenshot_capture;

use openzt2_interface_state::photos::photo_capture_types::CapturePhotoRequest;
use openzt2_interface_state::photos::photo_capture_types::CaptureScreenshotRequest;
use openzt2_interface_state::photos::photo_capture_types::ScreenshotCaptured;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeCompleted;

use photo_album_storage::{
    delete_requested_photos_and_release_unpersisted_images,
    initialize_default_photo_album_and_camera_roll_for_loaded_worlds,
};
use photo_album_types::DeletePhotoRequest;
use photo_capture::begin_requested_album_photo_captures;
use photo_capture_types::{PhotoCaptureFailed, PhotoCaptured, ScreenshotCaptureFailed};
use photo_challenge_hydration::hydrate_photo_challenges_and_sets_from_active_scenario_catalogue;
use photo_challenge_scoring::score_pending_photos_against_incomplete_lua_challenges;
use photo_subject_evidence_collection::collect_visible_photo_subjects_and_semantic_evidence_for_pending_captures;
use screenshot_capture::begin_requested_camera_screenshot_captures;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::ui::UiSet;
use openzt2_world_state::application_schedule::GameSet;

pub struct PhotosPlugin;

impl Plugin for PhotosPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<photo_configuration::PhotoConfiguration>()
            .init_asset_loader::<photo_configuration::PhotoConfigurationLoader>()
            .add_systems(Startup, (
                photo_configuration::load_photo_configuration,
                photo_challenge_goal_presentation::load_photo_challenge_item_fragment,
            ))
            .register_required_components::<
                openzt2_world_state::world_spawn::world_membership_types::WorldRoot,
                album_presentation::AlbumControlAvailability,
            >()
            .register_required_components::<
                openzt2_world_state::world_spawn::world_membership_types::WorldRoot,
                photo_caption::PhotoCaptionInterpreter,
            >()
            .init_resource::<photo_caption::PhotoCaptionSource>()
            .add_message::<CapturePhotoRequest>()
            .add_message::<CaptureScreenshotRequest>()
            .add_message::<ScreenshotCaptured>()
            .add_message::<ScreenshotCaptureFailed>()
            .add_message::<PhotoCaptured>()
            .add_message::<PhotoChallengeCompleted>()
            .add_message::<PhotoCaptureFailed>()
            .register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, photo_album_types::SelectedAlbumPhoto>()
            .register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, photo_album_types::PhotoPointerPress>()
            .add_message::<DeletePhotoRequest>()
            .add_observer(photo_album_storage::clear_retired_photo_selection)
            .add_observer(photo_mode_presentation_lifecycle::retire_photo_mode_presentation_when_control_is_removed)
            .add_observer(photo_mode_presentation_lifecycle::restore_photo_viewfinder_visibility_after_capture)
            .add_observer(photo_mode_presentation_lifecycle::hide_retired_photo_preview_layout)
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
                    (
                        begin_requested_album_photo_captures
                            .after(album_actions::route_photo_ui_actions)
                            .after(initialize_default_photo_album_and_camera_roll_for_loaded_worlds)
                            .after(photo_mode_presentation_lifecycle::show_authored_photo_mode_document_for_entered_photo_mode),
                        begin_requested_camera_screenshot_captures,
                    ).chain(),
                    photo_capture::expire_pending_album_photo_captures
                        .before(begin_requested_album_photo_captures),
                    delete_requested_photos_and_release_unpersisted_images
                        .after(album_actions::route_photo_ui_actions),
                    photo_mode_presentation_lifecycle::
                        show_authored_photo_mode_document_for_entered_photo_mode,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    album_presentation::update_album_control_availability_from_album_changes,
                    album_presentation::set_photo_album_control_interaction_availability_from_album_state
                        .after_ignore_deferred(album_presentation::update_album_control_availability_from_album_changes),
                )
                    .in_set(UiSet::DomainProjection)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    album_operations::sort_changed_album_photo_memberships
                        .before(album_presentation::present_photo_camera_roll_and_album_choices_in_authored_rows)
                        .before(album_presentation::present_active_photo_album_and_camera_roll_image_slots)
                        .before(album_presentation::present_stored_photo_captions),
                    album_presentation::present_photo_camera_roll_and_album_choices_in_authored_rows,
                    album_presentation::present_active_photo_album_and_camera_roll_image_slots,
                    album_presentation::present_active_photo_album_photo_count,
                    album_presentation::present_photo_album_and_photo_mode_camera_roll_text,
                    album_presentation::present_stored_photo_captions,
                    photo_mode_presentation_lifecycle::show_newly_captured_photo_in_authored_photo_mode_preview
                        .after(photo_mode_presentation_lifecycle::advance_and_retire_authored_last_captured_photo_preview),
                    photo_mode_presentation_lifecycle::advance_and_retire_authored_last_captured_photo_preview,
                    (
                        photo_mode_presentation_lifecycle::
                            initialize_authored_photo_mode_transient_surface_visibility_after_document_projection,
                        photo_challenge_goal_presentation::project_selected_photo_challenge_goals,
                    ).chain(),
                )
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            );
        app.add_systems(
            PostUpdate,
            (
                photo_mode_presentation_lifecycle::project_photo_viewfinder_capture_visibility
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
                collect_visible_photo_subjects_and_semantic_evidence_for_pending_captures
                    .after(bevy::transform::TransformSystems::Propagate)
                    .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            )
                .run_if(in_state(GamePhase::InGame)),
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
                .run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame))),
        );
        app.add_systems(
            Update,
            (
                photo_challenge_lua_virtual_machine_invalidation::invalidate_changed_photo_challenge_scorers,
                photo_caption::resolve_authored_photo_caption_script,
                photo_caption::generate_captions_once_for_captured_photos
                    .after(photo_caption::resolve_authored_photo_caption_script),
                score_pending_photos_against_incomplete_lua_challenges
                    .after(photo_challenge_lua_virtual_machine_invalidation::invalidate_changed_photo_challenge_scorers)
                    .after(photo_caption::generate_captions_once_for_captured_photos),
            )
                .after(hydrate_photo_challenges_and_sets_from_active_scenario_catalogue)
                .in_set(GameSet::Intent)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}

#[cfg(test)]
mod tests;
