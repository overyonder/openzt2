//! Boot, menus and world selection.

mod boot_splash_lifecycle;
mod campaign_selection_list_presentation;
mod exit_confirmation_presentation;
mod in_game_options_overlay_lifecycle;
mod in_game_shell_ui_action_routing;
mod loading_screen_presentation;
mod main_menu_screen_lifecycle_and_presentation;
mod main_mode_cash;
mod map_selection_screen_lifecycle;
mod menu_screen_lifecycle;
mod online_message_operations;
mod online_message_types;
mod profile_dialog_lifecycle;
mod profile_selection_list_presentation;
pub(crate) mod shell_navigation_and_world_launch;
pub mod shell_navigation_request_types;
mod shell_ui_action_routing;
#[cfg(test)]
mod tests;
mod world_choice_catalogue_hydration;
mod world_choice_list_presentation;
mod world_load_completion;
mod world_selection_presentation;
mod world_selection_presentation_types;

use bevy::prelude::*;
use menu_screen_lifecycle::ShellMenuPage;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::ShellNavigationAfterSettings;
use openzt2_world_state::application_schedule::GameSet;
use shell_navigation_request_types::{
    ChooseWorld, ExitApplication, NavigateShellBack, ReturnToMainMenu, SelectWorldSessionMode,
    ShowDownloads, ShowOptions, ShowSavedGames, SplashFinished, StartSelectedWorld,
};

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, main_mode_cash::load_main_mode_cash)
            .add_sub_state::<ShellMenuPage>()
            .add_message::<shell_navigation_request_types::ContinueCampaign>()
            .init_resource::<ShellSelection>()
            .init_resource::<shell_navigation_and_world_launch::SelectedModeDocument>()
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
                )
                    .chain()
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                online_message_operations::project_online_message_surface_visibility
                    .after(online_message_operations::poll_online_message_requests_and_create_authored_rows)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                OnEnter(GamePhase::Boot),
                boot_splash_lifecycle::create_boot_splash_screen_and_shell_cameras,
            )
            .add_systems(
                Update,
                boot_splash_lifecycle::reflect_game_phase_in_shared_menu_backdrop_visibility
                    .in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
            )
            .add_systems(OnEnter(GamePhase::MainMenu), shell_navigation_and_world_launch::clear_selected_mode_document)
            .add_systems(
                OnEnter(ShellMenuPage::MainMenu),
                main_menu_screen_lifecycle_and_presentation::
                    create_main_menu_screen,
            )
            .add_systems(
                OnEnter(GamePhase::MapSelection),
                map_selection_screen_lifecycle::
                    create_or_reuse_selected_mode_map_selection_screen,
            )
            .add_systems(
                OnEnter(GamePhase::Loading),
                loading_screen_presentation::show_authored_loading_screen,
            )
            .add_systems(
                Update,
                loading_screen_presentation::project_world_loading_progress
                    .in_set(openzt2_interface_state::ui::UiSet::DomainProjection)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                (
                    world_choice_catalogue_hydration::
                        hydrate_launchable_world_choices_from_active_scenario_catalogue,
                    campaign_selection_list_presentation::
                        request_authored_campaign_and_scenario_list_row_counts,
                    campaign_selection_list_presentation::
                        project_campaign_and_scenario_records_into_authored_list_rows
                        .after(world_choice_catalogue_hydration::hydrate_launchable_world_choices_from_active_scenario_catalogue)
                        .after_ignore_deferred(campaign_selection_list_presentation::request_authored_campaign_and_scenario_list_row_counts),
                    world_choice_list_presentation::
                        project_launchable_world_choices_into_authored_list_rows
                        .after(world_choice_catalogue_hydration::hydrate_launchable_world_choices_from_active_scenario_catalogue),
                    world_selection_presentation::
                        filter_world_choice_rows_by_selected_catalogue_filter
                        .after(world_choice_list_presentation::project_launchable_world_choices_into_authored_list_rows),
                    world_selection_presentation::
                        project_selected_world_catalogue_filter_into_authored_location_toggle,
                    world_selection_presentation::show_only_selected_secondary_globe_biome_model,
                    world_selection_presentation::
                        project_selected_world_facts_into_authored_map_selection_controls
                        .after(campaign_selection_list_presentation::project_campaign_and_scenario_records_into_authored_list_rows)
                        .after_ignore_deferred(world_selection_presentation::filter_world_choice_rows_by_selected_catalogue_filter),
                )
                    .after(shell_ui_action_routing::route_authored_shell_ui_actions_to_navigation_requests)
                    .in_set(openzt2_interface_state::ui::UiSet::Routing)
                    .run_if(in_state(GamePhase::MapSelection)),
            )
            .add_systems(
                Update,
                (
                    (
                        shell_navigation_and_world_launch::
                            select_requested_play_mode_and_enter_map_selection,
                        shell_navigation_and_world_launch::
                            select_world_from_activated_choice_view,
                        shell_navigation_and_world_launch::select_requested_compatible_world,
                        shell_navigation_and_world_launch::request_campaign_launch_after_world_exit,
                        shell_navigation_and_world_launch::begin_loading_selected_world,
                    ).chain_ignore_deferred()
                     .after_ignore_deferred(shell_ui_action_routing::route_authored_shell_ui_actions_to_navigation_requests),
                    in_game_shell_ui_action_routing::route_authored_in_game_shell_ui_actions,
                    in_game_options_overlay_lifecycle::close_finished_in_game_options_overlays
                        .after(in_game_shell_ui_action_routing::route_authored_in_game_shell_ui_actions),
                    exit_confirmation_presentation::
                        reveal_authored_exit_confirmation_after_modal_projection,
                    shell_navigation_and_world_launch::exit_application_after_request
                        .after_ignore_deferred(in_game_shell_ui_action_routing::route_authored_in_game_shell_ui_actions),
                    menu_screen_lifecycle::route_requested_shell_menu_page
                        .after_ignore_deferred(in_game_shell_ui_action_routing::route_authored_in_game_shell_ui_actions)
                        .after_ignore_deferred(shell_navigation_and_world_launch::begin_loading_selected_world),
                )
                    .in_set(ShellNavigationAfterSettings)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                shell_ui_action_routing::route_authored_shell_ui_actions_to_navigation_requests
                    .in_set(openzt2_interface_state::ui::UiSet::Routing)
                    .run_if(shell_is_active),
            )
            .add_systems(OnEnter(ShellMenuPage::Options), menu_screen_lifecycle::open_secondary_shell_menu)
            .add_systems(OnEnter(ShellMenuPage::Downloads), menu_screen_lifecycle::open_secondary_shell_menu)
            .add_systems(OnEnter(ShellMenuPage::SavedGames), menu_screen_lifecycle::open_secondary_shell_menu)
            .add_systems(
                OnEnter(GamePhase::MapSelection),
                shell_navigation_and_world_launch::begin_pending_campaign_launch_after_world_exit,
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
                    profile_dialog_lifecycle::close_profile_dialog_documents_once_their_dialog_hides,
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
