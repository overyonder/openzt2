pub(crate) mod admission;
mod animal_care_control_types;
mod animal_health_ui_action_routing;
mod behavior_rider_ejection;
mod first_person_mode_control_execution;
mod first_person_mode_control_operations;
mod first_person_view_angle_integration;
#[cfg(test)]
mod first_person_view_angle_integration_tests;
mod fossil_mode_control_execution;
mod free_camera_look_control;
mod guest_view_navigation_marker_projection;
#[cfg(test)]
mod guest_view_navigation_marker_projection_tests;
mod guest_view_navigation_marker_types;
pub(crate) mod immersive_control_execution_types;
mod session_lifecycle;
use openzt2_interface_state::immersive_modes::immersive_mode_control_types;
mod immersive_mode_entry_and_policy_operations;
pub(crate) mod immersive_mode_entry_reservation;
mod immersive_mode_entry_ui_action_routing;
mod immersive_mode_input_policy_operations;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types;
pub(crate) mod immersive_mode_policy_types;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types;
mod interaction_cursor_presentation;
mod interaction_prefab_presentation;
mod photo_mode_control_execution;
mod show_edit_mode_control_execution;
mod super_staff_avatar_start_position_search;
mod tranquilizer_control_execution;

use bevy::{
    ecs::schedule::common_conditions::{any_with_component, on_message},
    prelude::*,
};

use immersive_mode_message_types::{
    EnterImmersiveMode, ExitImmersiveMode, ImmersiveModeEntered, ImmersiveModeExited,
    ImmersiveModeRejected,
};
use immersive_mode_state_types::ActiveImmersiveMode;
use immersive_mode_state_types::ImmersiveMode;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::GameSet;

pub struct ImmersiveModesPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ImmersiveModeTransitions;

impl Plugin for ImmersiveModesPlugin {
    fn build(&self, app: &mut App) {
        session_lifecycle::register(app);
        app.add_message::<EnterImmersiveMode>()
            .add_message::<ExitImmersiveMode>()
            .add_message::<ImmersiveModeRejected>()
            .add_message::<ImmersiveModeEntered>()
            .add_message::<ImmersiveModeExited>()
            .add_observer(behavior_rider_ejection::eject_player_from_authored_subject)
            .add_observer(guest_view_navigation_marker_projection::drop_navigation_markers_when_guest_view_ends)
            .add_observer(interaction_cursor_presentation::remove_immersive_cursor_on_policy_removal)
            .add_observer(interaction_prefab_presentation::remove_immersive_prefab_on_policy_removal)
            .add_systems(
                Update,
                (
                    immersive_mode_entry_ui_action_routing::route_authored_immersive_mode_entry_actions_from_activated_ui_nodes
                        .before(crate::camera::CameraUiActionRouting),
                    immersive_mode_entry_reservation::reserve_one_pending_immersive_mode_entry_from_valid_controller_request
                        .after(crate::camera::CameraUiActionRouting),
                    (
                        admission::validate_guest.run_if(any_with_component::<admission::PendingMode<immersive_control_execution_types::GuestViewControl>>),
                        admission::validate_first_person.run_if(any_with_component::<admission::PendingMode<immersive_control_execution_types::FirstPersonControl>>),
                        admission::validate_staff.run_if(any_with_component::<admission::PendingMode<admission::StaffControl>>),
                        admission::validate_search.run_if(any_with_component::<admission::PendingMode<immersive_control_execution_types::FossilSearchControl>>),
                        admission::validate_assembly.run_if(any_with_component::<admission::PendingMode<immersive_control_execution_types::FossilAssemblyControl>>),
                        admission::validate_photo.run_if(any_with_component::<admission::PendingMode<immersive_mode_control_types::PhotoControl>>),
                        admission::validate_show.run_if(any_with_component::<admission::PendingMode<immersive_control_execution_types::ShowEditControl>>),
                        session_lifecycle::exit_requested_session.run_if(on_message::<ExitImmersiveMode>),
                    ).after(immersive_mode_entry_reservation::reserve_one_pending_immersive_mode_entry_from_valid_controller_request),
                )
                    .in_set(ImmersiveModeTransitions)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    animal_health_ui_action_routing::route_animal_health_tool_actions_from_activated_ui_nodes,
                    tranquilizer_control_execution::initialize_tranquilizer_tool
                        .after(animal_health_ui_action_routing::route_animal_health_tool_actions_from_activated_ui_nodes),
                )
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
                    tranquilizer_control_execution::sample_tranquilizer_aim,
                    tranquilizer_control_execution::apply_tranquilizer_input
                        .after(tranquilizer_control_execution::sample_tranquilizer_aim),
                )
                    .in_set(GameSet::Input)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                photo_mode_control_execution::project_live_photo_camera_view
                    .after(openzt2_world_state::application_schedule::WaterPresentationSet::View)
                    .in_set(GameSet::Presentation)
                    .run_if(active_mode_is(ImmersiveMode::Photo)),
            )
            .add_systems(
                Update,
                (
                    first_person_mode_control_execution::update_subject_relative_first_person_camera_transform
                        .run_if(active_view_mode),
                    guest_view_navigation_marker_projection::project_nearest_guest_navigation_positions_as_view_markers
                        .run_if(active_mode_is(ImmersiveMode::GuestView)),
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    interaction_cursor_presentation::project_changed_immersive_mode_cursor_images,
                    interaction_cursor_presentation::move_immersive_mode_cursor_visual_to_current_pointer_position
                        .after(interaction_cursor_presentation::project_changed_immersive_mode_cursor_images),
                    interaction_prefab_presentation::create_world_prefab_visual_when_immersive_mode_prefab_is_ready,
                )
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
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
