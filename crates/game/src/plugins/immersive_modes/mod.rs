mod active_immersive_mode_lifecycle;
pub(crate) mod animal_care_control_types;
mod animal_health_ui_action_routing;
mod extinct_animal_mode_entry_execution;
mod first_person_mode_control_execution;
mod first_person_mode_control_operations;
mod first_person_mode_entry_execution;
mod first_person_view_angle_integration;
#[cfg(test)]
mod first_person_view_angle_integration_tests;
mod fossil_mode_control_execution;
mod free_camera_look_control;
mod guest_view_navigation_marker_projection;
#[cfg(test)]
mod guest_view_navigation_marker_projection_tests;
mod guest_view_navigation_marker_types;
mod hud_visibility_policy_execution;
pub(crate) mod immersive_mode_control_types;
mod immersive_mode_entry_and_policy_operations;
mod immersive_mode_entry_reservation;
mod immersive_mode_entry_ui_action_routing;
mod immersive_mode_input_policy_operations;
pub(crate) mod immersive_mode_message_types;
pub(crate) mod immersive_mode_policy_types;
pub(crate) mod immersive_mode_state_types;
mod interaction_cursor_presentation;
mod interaction_prefab_presentation;
mod photo_mode_control_execution;
mod photo_mode_entry_execution;
mod show_edit_mode_control_execution;
mod show_edit_mode_entry_execution;
mod simulation_pause_policy_execution;
mod super_staff_avatar_start_position_search;
mod super_staff_mode_entry;
mod tranquilizer_control_execution;

use bevy::prelude::*;

use immersive_mode_message_types::{
    EnterImmersiveMode, ExitImmersiveMode, ImmersiveModeEntered, ImmersiveModeExited,
    ImmersiveModeRejected,
};
use immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode};

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::GameSet;

pub(super) struct ImmersiveModesPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ImmersiveModeTransitions;

impl Plugin for ImmersiveModesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EnterImmersiveMode>()
            .add_message::<ExitImmersiveMode>()
            .add_message::<ImmersiveModeRejected>()
            .add_message::<ImmersiveModeEntered>()
            .add_message::<ImmersiveModeExited>()
            .add_systems(
                Update,
                (
                    immersive_mode_entry_reservation::reserve_one_pending_immersive_mode_entry_from_valid_controller_request,
                    first_person_mode_entry_execution::enter_guest_view_for_pending_guest_subject,
                    first_person_mode_entry_execution::enter_first_person_mode_for_pending_navigable_subject,
                    super_staff_mode_entry::enter_pending_super_staff_mode,
                    extinct_animal_mode_entry_execution::enter_fossil_search_for_pending_unexhausted_recovery_site,
                    extinct_animal_mode_entry_execution::enter_fossil_assembly_for_pending_fossil_set_assembly,
                    photo_mode_entry_execution::enter_photo_mode_when_readback_is_available_and_idle,
                    show_edit_mode_entry_execution::enter_show_edit_for_pending_stage_with_information_panel,
                    immersive_mode_entry_reservation::reject_and_remove_uncommitted_pending_immersive_mode_entries,
                    immersive_mode_entry_ui_action_routing::route_authored_immersive_mode_entry_actions_from_activated_ui_nodes,
                    animal_health_ui_action_routing::route_animal_health_tool_actions_from_activated_ui_nodes,
                    tranquilizer_control_execution::initialize_tranquilizer_tool_from_active_world_definitions,
                    active_immersive_mode_lifecycle::request_immersive_mode_exit_when_subject_or_tool_is_removed,
                    active_immersive_mode_lifecycle::apply_requested_immersive_mode_exit_and_remove_owned_state,
                )
                    .chain()
                    .in_set(ImmersiveModeTransitions)
                    .after(crate::plugins::camera::CameraUiActionRouting)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    first_person_mode_control_execution::apply_guest_view_look_and_navigation_controls
                        .run_if(active_mode_is(ImmersiveMode::GuestView)),
                    first_person_mode_control_execution::apply_first_person_look_and_navigation_controls
                        .run_if(active_walking_first_person_mode),
                    fossil_mode_control_execution::discover_fossil_or_cancel_active_fossil_search_mode
                        .run_if(active_mode_is(ImmersiveMode::FossilSearch)),
                    fossil_mode_control_execution::cancel_active_fossil_assembly_mode
                        .run_if(active_mode_is(ImmersiveMode::FossilAssembly)),
                    free_camera_look_control::aim_active_free_immersive_camera,
                    photo_mode_control_execution::capture_or_cancel_active_photo_mode
                        .run_if(active_mode_is(ImmersiveMode::Photo)),
                    show_edit_mode_control_execution::cancel_active_show_edit_mode
                        .run_if(active_mode_is(ImmersiveMode::ShowEdit)),
                    tranquilizer_control_execution::aim_charge_fire_or_cancel_active_tranquilizer_controls,
                )
                    .in_set(GameSet::Input)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    first_person_mode_control_execution::update_subject_relative_first_person_camera_transform
                        .run_if(active_view_mode),
                    guest_view_navigation_marker_projection::project_nearest_guest_navigation_positions_as_view_markers
                        .run_if(active_mode_is(ImmersiveMode::GuestView)),
                    guest_view_navigation_marker_projection::retire_navigation_marker_presentations_without_guest_view_controllers,
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    simulation_pause_policy_execution::pause_simulation_while_requested_immersive_mode_is_active_and_restore_previous_state,
                    hud_visibility_policy_execution::hide_ingame_hud_while_requested_immersive_mode_is_active_and_restore_visibility,
                    interaction_cursor_presentation::create_cursor_visual_for_changed_immersive_mode_cursor_policy,
                    interaction_cursor_presentation::move_immersive_mode_cursor_visual_to_current_pointer_position,
                    interaction_prefab_presentation::create_world_prefab_visual_when_immersive_mode_prefab_is_ready,
                    interaction_cursor_presentation::retire_cursor_visuals_without_immersive_mode_cursor_owner,
                    interaction_prefab_presentation::retire_prefab_visuals_without_immersive_mode_prefab_owner,
                )
                    .chain()
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                (
                    active_immersive_mode_lifecycle::remove_active_immersive_mode_state_when_leaving_gameplay,
                    hud_visibility_policy_execution::restore_ingame_hud_visibility_when_leaving_gameplay,
                    interaction_cursor_presentation::retire_all_immersive_mode_cursor_visuals_when_leaving_gameplay,
                    interaction_prefab_presentation::retire_all_immersive_mode_prefab_visuals_when_leaving_gameplay,
                )
                    .chain(),
            );
    }
}

fn active_mode_is(mode: ImmersiveMode) -> impl Fn(Query<&ActiveImmersiveMode>) -> bool + Clone {
    move |active: Query<&ActiveImmersiveMode>| active.iter().any(|active| active.mode == mode)
}

fn active_walking_first_person_mode(active: Query<&ActiveImmersiveMode>) -> bool {
    active.iter().any(|active| {
        matches!(
            active.mode,
            ImmersiveMode::FirstPerson | ImmersiveMode::SuperStaff
        )
    })
}

fn active_view_mode(active: Query<&ActiveImmersiveMode>) -> bool {
    active.iter().any(|active| {
        matches!(
            active.mode,
            ImmersiveMode::GuestView
                | ImmersiveMode::FirstPerson
                | ImmersiveMode::SuperStaff
                | ImmersiveMode::FossilSearch
                | ImmersiveMode::FossilAssembly
        )
    })
}
