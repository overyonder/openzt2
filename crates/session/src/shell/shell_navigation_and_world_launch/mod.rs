use bevy::{app::AppExit, prelude::*};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::world_scenario::WorldMapSupportedGameModeFlags;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;

use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::shell_selection_types::WorldChoice;
use openzt2_interface_state::shell::shell_selection_types::WorldChoiceBinding;

use super::shell_navigation_request_types::{
    ChooseWorld, ExitApplication, SelectWorldSessionMode, StartSelectedWorld,
};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

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
    views: Query<WorldChoiceBinding>,
    choices: Query<&WorldChoice>,
    mut selection: ResMut<ShellSelection>,
) {
    for activation in activations.read() {
        if !matches!(activation.trigger, UiTrigger::Press | UiTrigger::Submit) {
            continue;
        }
        let Some(view) = views
            .get(activation.node)
            .ok()
            .and_then(|binding| binding.choice())
        else {
            continue;
        };
        let Ok(choice) = choices.get(view) else {
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

const DEFAULT_CHALLENGE_STARTING_CASH_CENTS: i64 = 3_000_000;

/// Clears the old world selection when switching play modes.
pub(super) fn select_requested_play_mode_and_enter_map_selection(
    mut selections: MessageReader<SelectWorldSessionMode>,
    mut selection: ResMut<ShellSelection>,
    mut next_phase: ResMut<NextState<GamePhase>>,
) {
    for SelectWorldSessionMode(mode) in selections.read() {
        selection.mode = Some(*mode);
        selection.scenario = None;
        selection.starting_cash_cents = match mode {
            WorldSessionMode::Challenge => Some(DEFAULT_CHALLENGE_STARTING_CASH_CENTS),
            _ => None,
        };
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
    profile_variables: Res<
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    >,
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
                    .filter(|scenario| {
                        profile_variables.scenario_state(scenario)
                            != openzt2_game_data::world_scenario::CampaignScenarioState::Locked
                    })
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
            saved_world_document: None,
            operation: None,
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

#[derive(Component)]
pub(super) struct PendingCampaignLaunch(openzt2_game_data::AssetId);

pub(super) fn request_campaign_launch_after_world_exit(
    mut commands: Commands,
    mut requests: MessageReader<super::shell_navigation_request_types::ContinueCampaign>,
    phase: Res<State<GamePhase>>,
    mut selection: ResMut<ShellSelection>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    variables: Res<
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    >,
) {
    let target = requests
        .read()
        .filter(|_| *phase.get() == GamePhase::InGame)
        .filter(|request| {
            scenarios
                .get(&assets)
                .and_then(|view| view.campaign_scenario(request.0))
                .is_some_and(|scenario| {
                    variables.scenario_state(scenario)
                        != openzt2_game_data::world_scenario::CampaignScenarioState::Locked
                })
        })
        .map(|request| request.0)
        .last();
    let Some(target) = target else { return };
    commands.spawn((
        PendingCampaignLaunch(target),
        DespawnOnEnter(GamePhase::MainMenu),
    ));
    selection.mode = Some(WorldSessionMode::Campaign);
    selection.scenario = Some(target);
    selection.starting_cash_cents = None;
    next_phase.set(GamePhase::MapSelection);
}

pub(super) fn begin_pending_campaign_launch_after_world_exit(
    mut commands: Commands,
    launches: Query<(Entity, &PendingCampaignLaunch)>,
    mut selection: ResMut<ShellSelection>,
    mut starts: MessageWriter<StartSelectedWorld>,
) {
    for (entity, launch) in &launches {
        selection.scenario = Some(launch.0);
        starts.write(StartSelectedWorld);
        commands.entity(entity).despawn();
    }
}
