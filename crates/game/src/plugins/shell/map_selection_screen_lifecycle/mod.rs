use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    game_session_types::WorldSessionMode,
    plugins::ui::ui_document_lifecycle_contracts::UiRoleRequests,
};

use super::{
    shell_screen_presentation_types::MainMenuBackdrop,
    shell_selection_types::{ShellScreen, ShellSelection},
    world_selection_presentation_types::SelectedWorldCatalogueFilter,
};

pub(super) fn create_or_reuse_selected_mode_map_selection_screen(
    mut commands: Commands,
    shell_selection: Res<ShellSelection>,
    existing_shell_screens: Query<(), With<ShellScreen>>,
    mut requested_ui_roles: ResMut<UiRoleRequests>,
) {
    if !existing_shell_screens.is_empty() {
        return;
    }
    let Some(selected_play_mode) = shell_selection.mode else {
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
