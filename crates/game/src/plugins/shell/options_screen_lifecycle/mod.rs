use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiRole;

use super::{
    shell_navigation_request_types::ShowOptions, shell_screen_presentation_types::MainMenuBackdrop,
    shell_selection_types::ShellScreen,
};

pub(super) fn replace_current_shell_screen_with_requested_options_screen(
    mut commands: Commands,
    mut options_screen_requests: MessageReader<ShowOptions>,
    current_shell_screens: Query<(Entity, &ShellScreen)>,
    mut requested_ui_roles: MessageWriter<ShowUiRole>,
) {
    for _ in options_screen_requests.read() {
        for (shell_screen, _) in &current_shell_screens {
            commands.entity(shell_screen).despawn();
        }
        let options_screen_owner = commands
            .spawn((ShellScreen::Options, Visibility::Inherited))
            .id();
        let main_menu_backdrop_owner = commands
            .spawn((
                MainMenuBackdrop,
                Visibility::Inherited,
                GlobalZIndex(-1),
                ChildOf(options_screen_owner),
            ))
            .id();
        requested_ui_roles.write(ShowUiRole {
            role: UiDocumentRole::MainMenuBackdrop,
            owner: main_menu_backdrop_owner,
        });
        requested_ui_roles.write(ShowUiRole {
            role: UiDocumentRole::Options,
            owner: options_screen_owner,
        });
    }
}
