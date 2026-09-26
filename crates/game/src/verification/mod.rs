mod verification_capture_target;
mod verification_fact_collection;
mod verification_game_phase_readiness;
mod verification_input_injection;
mod verification_journey_execution;
mod verification_journey_report;
mod verification_journey_script_parsing;
mod verification_journey_state_types;
pub(crate) mod verification_journey_types;
pub(crate) mod verification_launch_arguments;
pub(crate) mod verification_map_load;
mod verification_pointer_diagnostics;
mod verification_pointer_interaction_synchronization;
mod verification_ui_invariants;

use bevy::{picking::PickingSystems, prelude::*};

use crate::application_schedule::GameSet;

use verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneyRun,
};
use verification_journey_types::VerificationJourney;

pub(crate) struct VerificationJourneyPlugin {
    pub(crate) journey: VerificationJourney,
    pub(crate) output_directory: std::path::PathBuf,
    pub(crate) windowed: bool,
}

impl Plugin for VerificationJourneyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VerificationJourneyRun {
            journey: self.journey.clone(),
            output_directory: self.output_directory.clone(),
            windowed: self.windowed,
            capture_target: None,
            step_index: 0,
            step_frame: 0,
            step_pointer_position: None,
            step_resolved_frame: None,
            pointer_position: Vec2::ZERO,
            screenshot_pending: false,
            fact_collection: VerificationFactCollectionState::Idle,
            frame_work_started: None,
            measuring_frame_work: false,
            frame_time_milliseconds: Vec::new(),
            finished: false,
        })
        .init_resource::<VerificationJourneyReport>()
        // Authored image alternatives are otherwise random per screen; fixing the
        // choice keeps screenshots comparable between runs.
        .insert_resource(
            crate::plugins::ui::authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride::from_candidate_index(0),
        )
        .add_systems(
            First,
            verification_journey_execution::advance_verification_journey_by_one_frame
                .in_set(PickingSystems::Input),
        )
        .add_systems(
            PreUpdate,
            verification_pointer_interaction_synchronization::synchronize_bevy_ui_interaction_from_verification_pointer_input
                .in_set(PickingSystems::PostHover),
        )
        .add_systems(
            Update,
            (
                verification_capture_target::install_verification_capture_target_and_pointer_marker,
                verification_ui_invariants::record_broken_verification_ui_invariants,
                verification_pointer_diagnostics::log_world_pointer_and_placement_cursor_changes,
            )
                .after(GameSet::Presentation),
        )
        .add_systems(
            First,
            verification_journey_execution::mark_verification_frame_work_start
                .before(PickingSystems::Input),
        )
        .add_systems(
            Last,
            (
                verification_fact_collection::collect_requested_verification_facts,
                verification_journey_execution::record_verification_frame_work_time,
            )
                .chain(),
        )
        .add_observer(verification_pointer_diagnostics::log_verification_pointer_click_target);
    }
}
