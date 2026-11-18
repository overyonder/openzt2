use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_world_state::game_session_types::WorldSessionMode;

use openzt2_interface_state::shell::shell_selection_types::ShellScreen;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;

use super::world_selection_presentation_types::SelectedWorldCatalogueFilter;

#[derive(Component)]
pub(super) struct RetainedMapSelectionScreen;

pub(super) fn map_selection_screen_lifetime() -> DespawnWhen<GamePhase> {
    DespawnWhen::new(|transition| {
        !matches!(
            transition.entered,
            Some(GamePhase::Loading | GamePhase::MapSelection)
        )
    })
}

pub(super) fn create_or_reuse_selected_mode_map_selection_screen(
    mut commands: Commands,
    shell_selection: Res<ShellSelection>,
    retained_maps: Query<(), With<RetainedMapSelectionScreen>>,
    mut requested_ui_roles: MessageWriter<ShowUiRole>,
    mut next_phase: ResMut<NextState<openzt2_game_assets::application_lifecycle::GamePhase>>,
) {
    if !retained_maps.is_empty() {
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
            RetainedMapSelectionScreen,
            map_selection_screen_lifetime(),
            SelectedWorldCatalogueFilter::All,
            Visibility::Inherited,
        ))
        .id();
    requested_ui_roles.write(ShowUiRole {
        role: map_selection_document_role,
        owner: map_selection_screen_owner,
    });
}
