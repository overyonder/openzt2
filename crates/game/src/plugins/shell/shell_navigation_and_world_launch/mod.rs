use bevy::{app::AppExit, prelude::*};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::document::*;
use openzt2_game_data::world_scenario::WorldMapSupportedGameModeFlags;

use crate::application_lifecycle::GamePhase;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::persistence::profile_types::ProfileIndex;
use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiRole;
use crate::plugins::world_spawn::world_load_request_acceptance::BeginWorldLoad;

use super::{
    shell_navigation_request_types::{
        ChooseWorld, ExitApplication, NavigateShellBack, ReturnToMainMenu, SelectWorldSessionMode,
        ShowDownloads, ShowSavedGames, StartSelectedWorld,
    },
    shell_selection_types::{ShellScreen, ShellSelection, WorldChoice, WorldChoiceView},
};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Applies the focused application transition requested by gameplay or a
/// combined confirmation action. The current shell document is rebuilt only
/// when already in `MainMenu`; all other phases use Bevy state teardown.
pub(super) fn return_to_main_menu(
    mut commands: Commands,
    mut requests: MessageReader<ReturnToMainMenu>,
    phase: Res<State<GamePhase>>,
    screens: Query<(Entity, &ShellScreen)>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut show_ui: MessageWriter<ShowUiRole>,
) {
    for _ in requests.read() {
        if *phase.get() == GamePhase::MainMenu {
            replace_current_shell_screen(
                &mut commands,
                &screens,
                ShellScreen::MainMenu,
                UiDocumentRole::MainMenu,
                &mut show_ui,
            );
        } else {
            next_phase.set(GamePhase::MainMenu);
        }
    }
}

/// Requests a normal application exit.
pub(super) fn exit_application_after_request(
    mut requests: MessageReader<ExitApplication>,
    mut exit: MessageWriter<AppExit>,
) {
    if requests.read().next().is_some() {
        exit.write(AppExit::Success);
    }
}

/// Selects the world represented by the pressed or submitted choice.
pub(super) fn select_world_from_activated_choice_view(
    mut activations: MessageReader<UiNodeActivated>,
    views: Query<&WorldChoiceView>,
    choices: Query<&WorldChoice>,
    mut selection: ResMut<ShellSelection>,
) {
    for activation in activations.read() {
        if !matches!(activation.trigger, UiTrigger::Press | UiTrigger::Submit) {
            continue;
        }
        let Ok(view) = views.get(activation.node) else {
            continue;
        };
        let Ok(choice) = choices.get(view.0) else {
            continue;
        };
        if selection.mode == Some(choice.mode) {
            selection.scenario = Some(choice.scenario);
            debug!(
                ?choice.scenario,
                ?choice.mode,
                "selected authored world choice"
            );
        }
    }
}

/// Clears the old world selection when switching play modes.
pub(super) fn select_requested_play_mode_and_enter_map_selection(
    mut selections: MessageReader<SelectWorldSessionMode>,
    mut selection: ResMut<ShellSelection>,
    mut next_phase: ResMut<NextState<GamePhase>>,
) {
    for SelectWorldSessionMode(mode) in selections.read() {
        selection.mode = Some(*mode);
        selection.scenario = None;
        selection.starting_cash_cents = None;
        next_phase.set(GamePhase::MapSelection);
    }
}

pub(super) fn select_requested_compatible_world(
    mut requests: MessageReader<ChooseWorld>,
    mut selection: ResMut<ShellSelection>,
    worlds: Query<&WorldChoice>,
) {
    for ChooseWorld(scenario) in requests.read() {
        let compatible = selection.mode.is_some_and(|mode| {
            worlds
                .iter()
                .any(|choice| choice.scenario == *scenario && choice.mode == mode)
        });
        if compatible {
            selection.scenario = Some(*scenario);
        }
    }
}

pub(super) fn begin_loading_selected_world(
    mut requests: MessageReader<StartSelectedWorld>,
    phase: Res<State<GamePhase>>,
    selection: Res<ShellSelection>,
    profiles: Res<ProfileIndex>,
    active_scenarios: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut begin_load: MessageWriter<BeginWorldLoad>,
) {
    if *phase.get() != GamePhase::MapSelection {
        requests.clear();
        return;
    }
    for _ in requests.read() {
        let (Some(profile), Some(mode), Some(scenario)) = (
            profiles.selected_profile_identifier,
            selection.mode,
            selection.scenario,
        ) else {
            warn!(profile = ?profiles.selected_profile_identifier, mode = ?selection.mode, scenario = ?selection.scenario, "cannot start without a selected profile, mode, and world");
            continue;
        };
        let launchable = active_scenarios.get(&scenarios).is_some_and(|catalogue| {
            let map = match mode {
                WorldSessionMode::Campaign => catalogue
                    .campaign_scenario(scenario)
                    .filter(|_| catalogue.scenario(scenario).is_some())
                    .and_then(|scenario| catalogue.map(scenario.map)),
                WorldSessionMode::Freeform | WorldSessionMode::Challenge => catalogue.map(scenario),
            };
            map.is_some_and(|map| {
                supports_mode(map.supported_game_modes, mode) && catalogue.has_terrain(map.terrain)
            })
        });
        if !launchable {
            warn!(
                ?scenario,
                ?mode,
                "selected world is not launchable from the active scenario catalogue"
            );
            continue;
        }
        begin_load.write(BeginWorldLoad {
            scenario,
            mode,
            profile,
            starting_cash_cents: selection.starting_cash_cents,
        });
        next_phase.set(GamePhase::Loading);
    }
}

fn supports_mode(flags: WorldMapSupportedGameModeFlags, mode: WorldSessionMode) -> bool {
    let expected = match mode {
        WorldSessionMode::Freeform => WorldMapSupportedGameModeFlags::FREEFORM,
        WorldSessionMode::Challenge => WorldMapSupportedGameModeFlags::CHALLENGE,
        WorldSessionMode::Campaign => WorldMapSupportedGameModeFlags::CAMPAIGN,
    };
    flags.contains_all(expected)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn navigate_to_previous_shell_screen(
    mut commands: Commands,
    mut requests: MessageReader<NavigateShellBack>,
    phase: Res<State<GamePhase>>,
    screens: Query<(Entity, &ShellScreen)>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut show_ui: MessageWriter<ShowUiRole>,
) {
    for _ in requests.read() {
        match phase.get() {
            GamePhase::MapSelection => next_phase.set(GamePhase::MainMenu),
            GamePhase::MainMenu => {
                let nested_screen = screens.iter().any(|(_, screen)| {
                    matches!(
                        screen,
                        ShellScreen::ProfileSelect
                            | ShellScreen::Options
                            | ShellScreen::Downloads
                            | ShellScreen::SavedGames
                    )
                });
                if nested_screen {
                    replace_current_shell_screen(
                        &mut commands,
                        &screens,
                        ShellScreen::MainMenu,
                        UiDocumentRole::MainMenu,
                        &mut show_ui,
                    );
                }
            }
            _ => {}
        }
    }
}

pub(super) fn show_downloads_screen_from_requests(
    mut commands: Commands,
    mut requests: MessageReader<ShowDownloads>,
    screens: Query<(Entity, &ShellScreen)>,
    mut show_ui: MessageWriter<ShowUiRole>,
) {
    for _ in requests.read() {
        replace_current_shell_screen(
            &mut commands,
            &screens,
            ShellScreen::Downloads,
            UiDocumentRole::Downloads,
            &mut show_ui,
        );
    }
}

pub(super) fn show_saved_games_screen_from_requests(
    mut commands: Commands,
    mut requests: MessageReader<ShowSavedGames>,
    screens: Query<(Entity, &ShellScreen)>,
    mut show_ui: MessageWriter<ShowUiRole>,
) {
    for _ in requests.read() {
        replace_current_shell_screen(
            &mut commands,
            &screens,
            ShellScreen::SavedGames,
            UiDocumentRole::SavedGames,
            &mut show_ui,
        );
    }
}

pub(super) fn replace_current_shell_screen(
    commands: &mut Commands,
    screens: &Query<(Entity, &ShellScreen)>,
    screen: ShellScreen,
    role: UiDocumentRole,
    show_ui: &mut MessageWriter<ShowUiRole>,
) -> Entity {
    for (entity, _) in screens.iter() {
        commands.entity(entity).despawn();
    }
    let owner = commands.spawn((screen, Visibility::Inherited)).id();
    show_ui.write(ShowUiRole { role, owner });
    owner
}
