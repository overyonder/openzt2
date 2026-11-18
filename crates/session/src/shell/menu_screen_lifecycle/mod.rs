use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_interface_state::{
    shell::shell_selection_types::ShellScreen, ui::ui_document_lifecycle_contracts::ShowUiRole,
};

use super::shell_navigation_request_types::{
    NavigateShellBack, ReturnToMainMenu, ShowDownloads, ShowOptions, ShowSavedGames,
};

#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(GamePhase = GamePhase::MainMenu)]
pub(super) enum ShellMenuPage {
    #[default]
    MainMenu,
    Options,
    Downloads,
    SavedGames,
}

pub(super) fn route_requested_shell_menu_page(
    mut back: MessageReader<NavigateShellBack>,
    mut main_menu: MessageReader<ReturnToMainMenu>,
    mut options: MessageReader<ShowOptions>,
    mut downloads: MessageReader<ShowDownloads>,
    mut saved_games: MessageReader<ShowSavedGames>,
    phase: Res<State<GamePhase>>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut next_page: ResMut<NextState<ShellMenuPage>>,
) {
    let go_back = back.read().count() != 0;
    let go_to_main_menu = main_menu.read().count() != 0;
    let show_downloads = downloads.read().count() != 0;
    let show_saved_games = saved_games.read().count() != 0;
    let show_options = options.read().count() != 0;

    if *phase.get() != GamePhase::MainMenu {
        if go_to_main_menu || (go_back && *phase.get() == GamePhase::MapSelection) {
            next_phase.set(GamePhase::MainMenu);
        }
        return;
    }
    if show_options {
        next_page.set(ShellMenuPage::Options);
    } else if show_saved_games {
        next_page.set(ShellMenuPage::SavedGames);
    } else if show_downloads {
        next_page.set(ShellMenuPage::Downloads);
    } else if go_back || go_to_main_menu {
        next_page.set(ShellMenuPage::MainMenu);
    }
}

pub(super) fn open_secondary_shell_menu(
    mut commands: Commands,
    page: Res<State<ShellMenuPage>>,
    mut show: MessageWriter<ShowUiRole>,
) {
    let (screen, role) = match page.get() {
        ShellMenuPage::MainMenu => return,
        ShellMenuPage::Options => (ShellScreen::Options, UiDocumentRole::Options),
        ShellMenuPage::Downloads => (ShellScreen::Downloads, UiDocumentRole::Downloads),
        ShellMenuPage::SavedGames => (ShellScreen::SavedGames, UiDocumentRole::SavedGames),
    };
    let owner = commands
        .spawn((screen, Visibility::Inherited, DespawnOnExit(*page.get())))
        .id();
    show.write(ShowUiRole { role, owner });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    #[test]
    fn leaving_options_despawns_its_document_owner() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .insert_state(GamePhase::MainMenu)
            .add_sub_state::<ShellMenuPage>()
            .add_message::<ShowUiRole>()
            .add_systems(OnEnter(ShellMenuPage::Options), open_secondary_shell_menu);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<ShellMenuPage>>()
            .set(ShellMenuPage::Options);
        app.update();
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<ShellScreen>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .resource_mut::<NextState<ShellMenuPage>>()
            .set(ShellMenuPage::MainMenu);
        app.update();
        assert!(!app.world().entities().contains(owner));
    }
}
