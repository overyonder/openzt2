use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_interface_state::ui::ui_document_lifecycle_contracts::UiRoleRequests;
use openzt2_world_state::game_session_types::WorldSessionMode;

use openzt2_interface_state::shell::shell_screen_presentation_types::MainMenuBackdrop;
use openzt2_interface_state::shell::shell_selection_types::ShellScreen;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;

use super::world_selection_presentation_types::SelectedWorldCatalogueFilter;

pub(super) fn create_or_reuse_selected_mode_map_selection_screen(
    mut commands: Commands,
    shell_selection: Res<ShellSelection>,
    existing_shell_screens: Query<(), With<ShellScreen>>,
    mut requested_ui_roles: ResMut<UiRoleRequests>,
    mut next_phase: ResMut<NextState<openzt2_game_assets::application_lifecycle::GamePhase>>,
) {
    if !existing_shell_screens.is_empty() {
        return;
    }
    let Some(selected_play_mode) = shell_selection.mode else {
        // Loading a saved zoo from the main menu has no map-selection mode.
        // A failed restore must return to a screen that can actually be built.
        next_phase.set(openzt2_game_assets::application_lifecycle::GamePhase::MainMenu);
        return;
    };
    let (map_selection_document_role, map_selection_screen) =
        if selected_play_mode == WorldSessionMode::Campaign {
            (
                UiDocumentRole::MapSelect,
                ShellScreen::MapSelect {
                    mode: selected_play_mode,
                },
            )
        } else {
            (
                UiDocumentRole::Globe,
                ShellScreen::Globe {
                    mode: selected_play_mode,
                },
            )
        };
    let map_selection_screen_owner = commands
        .spawn((
            map_selection_screen,
            SelectedWorldCatalogueFilter::All,
            Visibility::Inherited,
        ))
        .id();
    let main_menu_backdrop_owner = commands
        .spawn((
            MainMenuBackdrop,
            Visibility::Inherited,
            GlobalZIndex(-1),
            ChildOf(map_selection_screen_owner),
        ))
        .id();
    requested_ui_roles.request(UiDocumentRole::MainMenuBackdrop, main_menu_backdrop_owner);
    requested_ui_roles.request(map_selection_document_role, map_selection_screen_owner);
}

pub(super) fn despawn_retained_map_selection_screen_after_world_activation(
    mut commands: Commands,
    shell_screens: Query<Entity, With<ShellScreen>>,
) {
    for shell_screen in &shell_screens {
        commands.entity(shell_screen).despawn();
    }
}
