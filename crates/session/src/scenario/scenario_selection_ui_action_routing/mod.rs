use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::UiScenarioAction;

use crate::shell::shell_navigation_request_types::ChooseWorld;
use crate::shell::shell_navigation_request_types::StartSelectedWorld;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::shell_selection_types::WorldChoice;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_world_state::game_session_types::WorldSessionMode;

use super::scenario_ui_types::{
    AuthoredScenarioUiActionDocumentQueries, ScenarioShellMessageWriters,
};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[allow(clippy::too_many_arguments)]
pub(super) fn route_scenario_selection_and_start_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_scenarios: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    mut selection: ResMut<ShellSelection>,
    (archives, asset_server, mut mode_document): (
        Res<openzt2_game_assets::asset_source::AssetArchives>,
        Res<AssetServer>,
        Option<ResMut<crate::shell::shell_navigation_and_world_launch::SelectedModeDocument>>,
    ),
    nodes: AuthoredScenarioUiActionDocumentQueries,
    parents: Query<&ChildOf>,
    values: Query<&UiValue>,
    mut choices: Query<(Entity, &WorldChoice, Option<&mut Visibility>)>,
    mut shell: ScenarioShellMessageWriters,
    continuation: Query<
        &super::scenario_lua_ui::presentation::ScenarioUiState,
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
) {
    let Some(scenarios) = active_scenarios.get(&scenarios) else {
        return;
    };
    for activation in activations.read() {
        let Ok((authored_actions, owner)) = nodes.action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = nodes.document_roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        for record in authored_actions.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiScenarioAction::PopulateScenarioSelection => {
                    for (entity, choice, visibility) in &mut choices {
                        let next = if selection.mode == Some(choice.mode) {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        };
                        if let Some(mut visibility) = visibility {
                            visibility.set_if_neq(next);
                        } else {
                            commands.entity(entity).insert(next);
                        }
                    }
                }
                UiScenarioAction::PopulateCampaignSelection => {
                    for (entity, choice, visibility) in &mut choices {
                        let next = if choice.mode == WorldSessionMode::Campaign {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        };
                        if let Some(mut visibility) = visibility {
                            visibility.set_if_neq(next);
                        } else {
                            commands.entity(entity).insert(next);
                        }
                    }
                }
                UiScenarioAction::PlayNextScenario => {
                    if let Some(target) = continuation
                        .single()
                        .ok()
                        .and_then(|state| state.continuation)
                    {
                        shell.continue_campaign.write(
                            crate::shell::shell_navigation_request_types::ContinueCampaign(target),
                        );
                    }
                }
                UiScenarioAction::SelectionChanged { scenario_path } => {
                    if let (Some(path), Some(mode)) =
                        (scenario_path.as_ref(), mode_document.as_mut())
                    {
                        let source = std::path::Path::new(path);
                        mode.0 = archives
                            .resolve_source_reference(source, path)
                            .map(|resolved| asset_server.load(resolved));
                    }
                    if let Ok((_, choice, _)) = choices.get(activation.node) {
                        shell.choose_world.write(ChooseWorld(choice.scenario));
                    }
                }
                UiScenarioAction::ClearSelection => {
                    selection.scenario = None;
                }
                UiScenarioAction::SetStartingCash => {
                    let authored_starting_cash_cents =
                        std::iter::successors(Some(activation.node), |entity| {
                            parents.get(*entity).ok().map(ChildOf::parent)
                        })
                        .find_map(|entity| values.get(entity).ok())
                        .map(|value| value.0.saturating_mul(100));
                    selection.starting_cash_cents = authored_starting_cash_cents.or_else(|| {
                        selection.scenario.and_then(|scenario| {
                            scenarios
                                .campaign_scenario(scenario)
                                .map(|scenario| scenario.starting_cash_cents)
                        })
                    });
                }
                UiScenarioAction::PlayTutorial => {
                    if let Ok((_, choice, _)) = choices.get(activation.node) {
                        selection.mode = Some(choice.mode);
                        selection.scenario = Some(choice.scenario);
                    }
                    if selection.mode.is_some() && selection.scenario.is_some() {
                        shell.start_world.write(StartSelectedWorld);
                    }
                }
                _ => {}
            }
        }
    }
}
