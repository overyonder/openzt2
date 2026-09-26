//! Boot, menus and world selection.

mod boot_splash_lifecycle;
mod campaign_selection_list_presentation;
mod exit_confirmation_presentation;
mod in_game_options_overlay_lifecycle;
mod in_game_shell_ui_action_routing;
mod loading_screen_presentation;
mod main_menu_screen_lifecycle_and_presentation;
mod map_selection_screen_lifecycle;
mod online_message_operations;
mod online_message_types;
mod options_screen_lifecycle;
mod post_save_navigation;
mod profile_selection_list_presentation;
mod shell_navigation_and_world_launch;
pub mod shell_navigation_request_types;
pub(crate) mod shell_screen_presentation_types;
pub mod shell_selection_types;
mod shell_ui_action_routing;
#[cfg(test)]
mod tests;
mod world_choice_catalogue_hydration;
mod world_choice_list_presentation;
mod world_load_completion;
mod world_selection_presentation;
mod world_selection_presentation_types;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::GameSet;
use shell_navigation_request_types::{
    ChooseWorld, ExitApplication, NavigateShellBack, ReturnToMainMenu, SelectWorldSessionMode,
    ShowDownloads, ShowOptions, ShowSavedGames, SplashFinished, StartSelectedWorld,
};
use shell_selection_types::ShellSelection;

pub struct ShellPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ShellNavigationAfterSettings;

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShellSelection>()
            .add_message::<NavigateShellBack>()
            .add_message::<ShowOptions>()
            .add_message::<ShowDownloads>()
            .add_message::<ShowSavedGames>()
            .add_message::<SelectWorldSessionMode>()
            .add_message::<ChooseWorld>()
            .add_message::<StartSelectedWorld>()
            .add_message::<ReturnToMainMenu>()
            .add_message::<ExitApplication>()
            .add_message::<SplashFinished>()
            .add_systems(
                Update,
                (
                    online_message_operations::start_online_message_request_for_newly_projected_surfaces,
                    online_message_operations::poll_online_message_requests_and_create_authored_rows,
                    online_message_operations::bind_displayed_online_message_to_authored_row_text,
                    online_message_operations::project_online_message_policy_and_content_availability_to_surface,
                )
                    .chain()
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                OnEnter(GamePhase::Boot),
                boot_splash_lifecycle::create_boot_splash_screen_and_shell_cameras,
            )
            .add_systems(
                OnEnter(GamePhase::MainMenu),
                main_menu_screen_lifecycle_and_presentation::
                    create_main_menu_screen_and_request_profile_index,
            )
            .add_systems(
                OnEnter(GamePhase::MapSelection),
                map_selection_screen_lifecycle::
                    create_or_reuse_selected_mode_map_selection_screen,
            )
            .add_systems(
                OnExit(GamePhase::MainMenu),
                main_menu_screen_lifecycle_and_presentation::
                    despawn_shell_screens_when_leaving_main_menu,
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                in_game_options_overlay_lifecycle::
                    close_in_game_options_overlays_when_game_exits,
            )
            .add_systems(
                OnEnter(GamePhase::Loading),
                loading_screen_presentation::show_authored_loading_screen,
            )
            .add_systems(
                OnExit(GamePhase::Loading),
                loading_screen_presentation::close_authored_loading_screen,
            )
            .add_systems(
                Update,
                loading_screen_presentation::project_world_loading_progress
                    .in_set(crate::plugins::ui::UiSet::DomainProjection)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                OnEnter(GamePhase::InGame),
                map_selection_screen_lifecycle::
                    despawn_retained_map_selection_screen_after_world_activation,
            )
            .add_systems(
                Update,
                (
                    world_choice_catalogue_hydration::
                        hydrate_launchable_world_choices_from_active_scenario_catalogue,
                    campaign_selection_list_presentation::
                        request_authored_campaign_and_scenario_list_row_counts,
                    campaign_selection_list_presentation::
                        project_campaign_and_scenario_records_into_authored_list_rows,
                    world_choice_list_presentation::
                        project_launchable_world_choices_into_authored_list_rows,
                    world_selection_presentation::
                        filter_world_choice_rows_by_selected_catalogue_filter,
                    world_selection_presentation::
                        project_selected_world_catalogue_filter_into_authored_location_toggle,
                    world_selection_presentation::show_only_selected_secondary_globe_biome_model,
                    world_selection_presentation::
                        project_selected_world_facts_into_authored_map_selection_controls,
                )
                    .chain()
                    .in_set(crate::plugins::ui::UiSet::Routing)
                    .run_if(in_state(GamePhase::MapSelection)),
            )
            .add_systems(
                Update,
                (
                    shell_navigation_and_world_launch::
                        select_requested_play_mode_and_enter_map_selection,
                    shell_navigation_and_world_launch::
                        select_world_from_activated_choice_view,
                    shell_navigation_and_world_launch::select_requested_compatible_world,
                    shell_navigation_and_world_launch::begin_loading_selected_world,
                    in_game_shell_ui_action_routing::route_authored_in_game_shell_ui_actions,
                    in_game_options_overlay_lifecycle::
                        close_hidden_in_game_options_overlays_and_restore_simulation_pause_state,
                    exit_confirmation_presentation::
                        reveal_authored_exit_confirmation_after_modal_projection,
                    post_save_navigation::
                        complete_or_cancel_pending_navigation_after_save_result,
                    shell_navigation_and_world_launch::return_to_main_menu,
                    shell_navigation_and_world_launch::exit_application_after_request,
                    shell_navigation_and_world_launch::navigate_to_previous_shell_screen,
                )
                    .chain()
                    .in_set(ShellNavigationAfterSettings)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                shell_ui_action_routing::route_authored_shell_ui_actions_to_navigation_requests
                    .in_set(crate::plugins::ui::UiSet::Routing)
                    .run_if(shell_is_active),
            )
            .add_systems(
                Update,
                (
                    shell_navigation_and_world_launch::show_downloads_screen_from_requests,
                    shell_navigation_and_world_launch::show_saved_games_screen_from_requests,
                )
                    .chain()
                    .after(
                        shell_navigation_and_world_launch::navigate_to_previous_shell_screen,
                    )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                options_screen_lifecycle::replace_current_shell_screen_with_requested_options_screen
                    .in_set(GameSet::Intent)
                    .run_if(shell_is_active),
            )
            .add_systems(
                Update,
                (
                    main_menu_screen_lifecycle_and_presentation::
                        project_application_version_into_new_authored_text_bindings,
                    main_menu_screen_lifecycle_and_presentation::
                        remove_fallback_main_menu_logo_after_document_projection,
                    profile_selection_list_presentation::
                        project_selected_profile_name_into_new_main_menu_nodes,
                    profile_selection_list_presentation::
                        refresh_selected_profile_name_after_profile_index_changes,
                    profile_selection_list_presentation::
                        request_authored_profile_selection_list_row_count,
                    profile_selection_list_presentation::
                        project_profile_index_entries_into_authored_rows,
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::MainMenu)),
            )
            .add_systems(
                Update,
                boot_splash_lifecycle::
                    enter_main_menu_after_splash_completion_or_ui_document_asset_load_failure
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Boot)),
            )
            .add_systems(
                Update,
                world_load_completion::enter_game_or_return_to_map_selection_after_world_load_result
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            );
    }
}

fn shell_is_active(phase: Res<State<GamePhase>>) -> bool {
    matches!(
        phase.get(),
        GamePhase::Boot | GamePhase::MainMenu | GamePhase::MapSelection
    )
}
