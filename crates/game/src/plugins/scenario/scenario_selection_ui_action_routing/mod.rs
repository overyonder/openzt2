use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::UiScenarioAction;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::shell::shell_navigation_request_types::ChooseWorld;
use crate::plugins::shell::shell_navigation_request_types::StartSelectedWorld;
use crate::plugins::shell::shell_selection_types::ShellSelection;
use crate::plugins::shell::shell_selection_types::WorldChoice;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;

use super::scenario_ui_types::{
    AuthoredScenarioUiActionDocumentQueries, ScenarioShellMessageWriters,
};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[allow(clippy::too_many_arguments)]
pub(super) fn route_scenario_selection_and_start_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_scenarios: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    mut selection: ResMut<ShellSelection>,
    nodes: AuthoredScenarioUiActionDocumentQueries,
    parents: Query<&ChildOf>,
    values: Query<&UiValue>,
    choices: Query<(Entity, &WorldChoice)>,
    mut shell: ScenarioShellMessageWriters,
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
                    for (entity, choice) in &choices {
                        commands
                            .entity(entity)
                            .insert(if selection.mode == Some(choice.mode) {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            });
                    }
                }
                UiScenarioAction::PopulateCampaignSelection => {
                    for (entity, choice) in &choices {
                        commands.entity(entity).insert(
                            if choice.mode == WorldSessionMode::Campaign {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            },
                        );
                    }
                }
                UiScenarioAction::PlayNextScenario => {
                    if let Some(current_scenario) = selection.scenario {
                        let next_scenario = scenarios.campaigns().find_map(|campaign| {
                            campaign
                                .scenarios
                                .windows(2)
                                .find(|pair| pair[0].id == current_scenario)
                                .map(|pair| pair[1].id)
                        });
                        if let Some(next_scenario) = next_scenario {
                            shell.choose_world.write(ChooseWorld(next_scenario));
                            shell.start_world.write(StartSelectedWorld);
                        }
                    }
                }
                UiScenarioAction::SelectionChanged => {
                    if let Ok((_, choice)) = choices.get(activation.node) {
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
                    if let Ok((_, choice)) = choices.get(activation.node) {
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
