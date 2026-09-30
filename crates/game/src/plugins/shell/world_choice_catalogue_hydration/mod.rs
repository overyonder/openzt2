use bevy::prelude::*;
use openzt2_game_data::{world_scenario::WorldMapSupportedGameModeFlags, AssetId};

use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;

use super::shell_selection_types::{
    GlobeMarker, ShellScreen, ShellSelection, WorldChoice, WorldChoiceView,
};

pub(super) fn hydrate_launchable_world_choices_from_active_scenario_catalogue(
    mut commands: Commands,
    selection: Res<ShellSelection>,
    active_catalogue: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    screens: Query<(Entity, Ref<ShellScreen>)>,
    existing_choices: Query<(Entity, &WorldChoice, &GlobeMarker, &ChildOf)>,
    views: Query<(Entity, &WorldChoiceView, Option<&ChildOf>, Has<UiListRow>)>,
) {
    if !active_catalogue.is_changed()
        && !catalogues.is_changed()
        && !selection.is_changed()
        && screens.iter().all(|(_, screen)| !screen.is_added())
    {
        return;
    }
    let Some(mode) = selection.mode else { return };
    let Some(catalogue) = active_catalogue.get(&catalogues) else {
        return;
    };
    for (owner, screen) in &screens {
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
        for (entity, current, marker, parent) in &existing_choices {
            if parent.parent() != owner
                || choices.iter().any(|(scenario, map, position)| {
                    current.mode == mode
                        && current.scenario == *scenario
                        && current.map == map.id
                        && marker.0 == *position
                })
            {
                continue;
            }
            for (view_entity, view, parent, is_list_row) in &views {
                if !is_list_row
                    && view.0 == entity
                    && !parent.is_some_and(|parent| {
                        views
                            .get(parent.parent())
                            .is_ok_and(|(_, parent_view, _, _)| parent_view.0 == entity)
                    })
                {
                    commands.entity(view_entity).despawn();
                }
            }
            commands.entity(entity).despawn();
        }
        for (scenario, map, globe_marker) in &choices {
            if existing_choices.iter().any(|(_, choice, marker, parent)| {
                parent.parent() == owner
                    && choice.mode == mode
                    && choice.scenario == *scenario
                    && choice.map == map.id
                    && marker.0 == *globe_marker
            }) {
                continue;
            }
            commands.spawn((
                WorldChoice {
                    scenario: *scenario,
                    map: AssetId(map.id.0),
                    mode,
                },
                GlobeMarker(*globe_marker),
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
