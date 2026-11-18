use std::collections::HashSet;

use bevy::prelude::*;
use openzt2_game_data::{world_scenario::WorldMapSupportedGameModeFlags, AssetId};

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::game_session_types::WorldSessionMode;

use openzt2_interface_state::shell::shell_selection_types::GlobeMarker;
use openzt2_interface_state::shell::shell_selection_types::ShellScreen;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::shell_selection_types::WorldChoice;

pub(super) fn hydrate_launchable_world_choices_from_active_scenario_catalogue(
    mut commands: Commands,
    selection: Res<ShellSelection>,
    active_catalogue: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    screens: Query<(Entity, Ref<ShellScreen>, Option<&Children>)>,
    existing_choices: Query<(&WorldChoice, &GlobeMarker)>,
) {
    if !active_catalogue.is_changed()
        && !catalogues.is_changed()
        && !selection.is_changed()
        && screens.iter().all(|(_, screen, _)| !screen.is_added())
    {
        return;
    }
    let Some(mode) = selection.mode else { return };
    let Some(catalogue) = active_catalogue.get(&catalogues) else {
        return;
    };
    for (owner, screen, children) in &screens {
        if !matches!(
            *screen,
            ShellScreen::Globe { .. } | ShellScreen::MapSelect { .. }
        ) {
            continue;
        }
        let choices = match mode {
            WorldSessionMode::Campaign => catalogue
                .campaigns()
                .flat_map(|campaign| &campaign.scenarios)
                .filter(|scenario| catalogue.scenario(scenario.id).is_some())
                .filter_map(|scenario| catalogue.map(scenario.map).map(|map| (scenario.id, map)))
                .collect::<Vec<_>>(),
            WorldSessionMode::Freeform | WorldSessionMode::Challenge => catalogue
                .maps()
                .map(|map| (AssetId(map.id.0), map))
                .collect::<Vec<_>>(),
        };
        let choices = choices
            .into_iter()
            .filter(|(_, map)| {
                authored_game_mode_flags_support_selected_play_mode(map.supported_game_modes, mode)
                    && catalogue.has_terrain(AssetId(map.terrain.0))
            })
            .filter_map(|(scenario, map)| {
                catalogue
                    .location(map.location)
                    .map(|location| (scenario, map, location.globe_marker))
            })
            .collect::<Vec<_>>();
        let key = |scenario: AssetId, map: AssetId, marker: [i16; 2]| (scenario, map, marker);
        let desired = choices
            .iter()
            .map(|(scenario, map, marker)| key(*scenario, map.id, *marker))
            .collect::<HashSet<_>>();
        let mut retained = HashSet::new();
        for child in children.into_iter().flatten().copied() {
            let Ok((current, marker)) = existing_choices.get(child) else {
                continue;
            };
            let choice = key(current.scenario, current.map, marker.0);
            if current.mode != mode || !desired.contains(&choice) || !retained.insert(choice) {
                commands.entity(child).despawn();
            }
        }
        for (scenario, map, globe_marker) in choices {
            if !retained.insert(key(scenario, map.id, globe_marker)) {
                continue;
            }
            commands.spawn((
                WorldChoice {
                    scenario,
                    map: AssetId(map.id.0),
                    mode,
                },
                GlobeMarker(globe_marker),
                ChildOf(owner),
            ));
        }
    }
}

fn authored_game_mode_flags_support_selected_play_mode(
    authored_game_mode_flags: WorldMapSupportedGameModeFlags,
    selected_play_mode: WorldSessionMode,
) -> bool {
    let expected_game_mode_flag = match selected_play_mode {
        WorldSessionMode::Freeform => WorldMapSupportedGameModeFlags::FREEFORM,
        WorldSessionMode::Challenge => WorldMapSupportedGameModeFlags::CHALLENGE,
        WorldSessionMode::Campaign => WorldMapSupportedGameModeFlags::CAMPAIGN,
    };
    authored_game_mode_flags.contains_all(expected_game_mode_flag)
}
