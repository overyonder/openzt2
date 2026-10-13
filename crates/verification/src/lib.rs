//! Runtime verification: scripted journeys and map loads that drive the game
//! through its own input, then record facts, logs and screenshots.

mod verification_animal_drinking_facts;
mod verification_animal_preview_diagnostics;
mod verification_animal_task_facts;
mod verification_building_panel_facts;
mod verification_capture_execution;
mod verification_capture_readiness;
mod verification_capture_target;
mod verification_catalogue_selection_facts;
mod verification_catalogue_wheel_diagnostics;
mod verification_catalogue_wheel_input;
mod verification_fact_collection;
mod verification_fixed_transform_facts;
mod verification_font_atlas_facts;
mod verification_game_phase_readiness;
mod verification_gamepad_input;
mod verification_guest_activity_facts;
mod verification_immersive_entry_facts;
mod verification_input_injection;
mod verification_journey_execution;
mod verification_journey_report;
mod verification_journey_script_parsing;
#[cfg(test)]
mod verification_journey_script_parsing_tests;
mod verification_journey_state_types;
pub mod verification_journey_types;
pub mod verification_launch_arguments;
pub mod verification_map_load;
mod verification_mod_manager_facts;
mod verification_object_placement_facts;
mod verification_object_placement_headroom_diagnostics;
mod verification_pointer_diagnostics;
mod verification_pointer_reach;
mod verification_profile_list_facts;
mod verification_research_facts;
mod verification_sale_confirmation_facts;
mod verification_save_navigation_facts;
mod verification_save_pause_facts;
mod verification_scenario_goal_facts;
mod verification_scenario_text_layout_diagnostics;
mod verification_session_navigation;
mod verification_staff_gate_facts;
mod verification_staff_gate_topology_diagnostics;
mod verification_text_edit_render_facts;
mod verification_ui_invariants;
mod verification_zoopedia_facts;

use bevy::{picking::PickingSystems, prelude::*};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::GameSet;

use verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
    VerificationSessionStage,
};
use verification_journey_types::{
    VerificationCaptureMode, VerificationJourney, VerificationNavigationRoutes,
};

pub struct VerificationJourneyPlugin {
    pub journeys: Box<[VerificationJourney]>,
    pub routes: VerificationNavigationRoutes,
    pub output_root: std::path::PathBuf,
    pub windowed: bool,
    pub capture_mode: VerificationCaptureMode,
    pub budget: std::time::Duration,
}

impl Plugin for VerificationJourneyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VerificationJourneySession {
            journeys: self.journeys.clone(),
            routes: self.routes.clone(),
            output_root: self.output_root.clone(),
            windowed: self.windowed,
            capture_target: None,
            journey_index: 0,
            stage: VerificationSessionStage::ChoosingRoute,
            steps: Box::default(),
            step_index: 0,
            step_frame: 0,
            step_started_at: std::time::Instant::now(),
            run_deadline: std::time::Instant::now() + self.budget,
            budget: self.budget,
            budget_exhausted: false,
            frame_rate_window: None,
            step_pointer_position: None,
            step_resolved_frame: None,
            step_target: None,
            step_alternative_targets: Vec::new(),
            step_target_reaims: 0,
            step_dismissal: default(),
            step_steady_value: None,
            last_primary_press: None,
            pointer_position: Vec2::ZERO,
            step_capture: default(),
            fact_collection: VerificationFactCollectionState::Idle,
            frame_work_started: None,
            measuring_frame_work: false,
            frame_time_milliseconds: Vec::new(),
            place: None,
            world_loads_started: 0,
            observed_phase: GamePhase::Boot,
            known_states: default(),
            frames_run: 0,
            journey_started_at: None,
            any_journey_failed: false,
            finished: false,
        })
        .init_resource::<VerificationJourneyReport>()
        .init_resource::<openzt2_zoo_state::placement::object_placement_headroom_diagnostic_types::ObjectPlacementHeadroomDiagnosticState>()
        .init_resource::<verification_save_pause_facts::SavePauseEvidence>()
        // Authored image alternatives are otherwise random per screen; fixing the
        // choice keeps screenshots comparable between runs.
        .insert_resource(
            openzt2_ui::authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride::from_candidate_index(0),
        )
        .add_systems(
            First,
            verification_journey_execution::advance_verification_journey_by_one_frame
                .in_set(PickingSystems::Input),
        )
        .add_systems(
            First,
            verification_gamepad_input::disconnect_journey_gamepads_after_completion_or_failure
                .after(verification_journey_execution::advance_verification_journey_by_one_frame),
        )
        .add_systems(
            Last,
            verification_gamepad_input::observe_journey_controller_requests_and_activations
                .before(verification_fact_collection::collect_requested_verification_facts),
        )
        .add_systems(
            Last,
            verification_font_atlas_facts::collect_font_atlas_verification_facts
                .after(verification_fact_collection::collect_requested_verification_facts)
                .before(verification_journey_execution::record_verification_frame_work_time),
        )
        .add_systems(
            Last,
            verification_immersive_entry_facts::collect_walking_mode_entry_verification_facts
                .after(verification_fact_collection::collect_requested_verification_facts)
                .before(verification_journey_execution::record_verification_frame_work_time),
        )
        .add_systems(
            Update,
            verification_object_placement_headroom_diagnostics::log_changed_object_placement_headroom_result
                .after(openzt2_zoo_state::placement::ObjectPlacementPreviewUpdateSet),
        )
        .add_systems(
            Update,
            verification_save_pause_facts::observe_authored_save_pause_transfer_start
                .in_set(openzt2_interface_state::ui::UiSet::Routing),
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
                verification_building_panel_facts::collect_building_panel_verification_facts,
                verification_fixed_transform_facts::collect_fixed_transform_verification_facts,
                verification_profile_list_facts::collect_profile_list_verification_facts,
                verification_profile_list_facts::collect_main_menu_profile_text_verification_facts,
                verification_mod_manager_facts::collect_mod_manager_verification_facts,
                verification_object_placement_facts::collect_object_placement_verification_facts,
                verification_staff_gate_facts::collect_staff_gate_verification_facts,
                verification_research_facts::collect_research_verification_facts,
                verification_catalogue_wheel_input::collect_catalogue_scroll_verification_facts,
                verification_catalogue_selection_facts::collect_catalogue_selection_verification_facts,
                verification_animal_task_facts::collect_animal_task_verification_facts,
                verification_animal_drinking_facts::collect_animal_drinking_verification_facts,
                verification_guest_activity_facts::collect_guest_activity_verification_facts,
                verification_save_navigation_facts::collect_save_navigation_verification_facts,
                verification_save_pause_facts::collect_save_pause_verification_facts,
                verification_gamepad_input::collect_journey_controller_verification_facts,
                verification_journey_execution::record_verification_frame_work_time,
            )
                .chain(),
        )
        .add_observer(verification_pointer_diagnostics::log_verification_pointer_click_target);
        app.init_resource::<verification_staff_gate_facts::VerificationStaffGateEvidence>()
            .add_systems(
                FixedUpdate,
                verification_staff_gate_facts::observe_keeper_routes_before_gate_operation
                    .after(openzt2_world_state::application_schedule::FixedGameSet::Navigate)
                    .before(openzt2_world_state::application_schedule::FixedGameSet::Act),
            );
        app.init_resource::<verification_animal_drinking_facts::AnimalDrinkingEvidence>()
            .add_systems(
                FixedUpdate,
                verification_animal_drinking_facts::observe_drinking_before_behavior_execution
                    .after(openzt2_world_state::application_schedule::FixedGameSet::Clock)
                    .before(openzt2_world_state::application_schedule::FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                verification_animal_drinking_facts::observe_drinking_after_behavior_effects
                    .after(openzt2_world_state::application_schedule::FixedGameSet::Act)
                    .before(openzt2_world_state::application_schedule::FixedGameSet::Economy),
            );
        verification_capture_readiness::install_verification_capture_readiness(
            app,
            self.capture_mode,
        );
        app.add_systems(
            Last,
            verification_scenario_text_layout_diagnostics::log_changed_scenario_text_layout,
        );
        app.add_systems(
            Last,
            verification_animal_preview_diagnostics::log_selected_animal_preview_at_capture_checkpoints
                .before(verification_journey_execution::record_verification_frame_work_time),
        );
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::ExtractSchedule,
            verification_text_edit_render_facts::collect_extracted_named_text_edit_verification_facts
                .after(openzt2_ui::authored_visual_layer_draw_order::place_authored_aspect_draws_in_owner_stack_slot),
        );
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::ExtractSchedule,
            verification_sale_confirmation_facts::collect_extracted_sale_confirmation_verification_facts
                .after(openzt2_ui::authored_text_physical_glyph_projection::preserve_authored_font_aspect_in_extracted_text_draws),
        );
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::ExtractSchedule,
            verification_scenario_goal_facts::collect_extracted_scenario_goal_verification_facts
                .after(openzt2_ui::authored_text_physical_glyph_projection::preserve_authored_font_aspect_in_extracted_text_draws),
        );
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::ExtractSchedule,
            verification_zoopedia_facts::collect_extracted_zoopedia_verification_facts
                .after(openzt2_ui::authored_text_physical_glyph_projection::preserve_authored_font_aspect_in_extracted_text_draws),
        );
        verification_staff_gate_topology_diagnostics::register_staff_gate_topology_diagnostics(app);
        verification_catalogue_wheel_diagnostics::register_catalogue_wheel_diagnostics(app);
    }
}
