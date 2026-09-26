//! Authored earthquake UI actions applied directly to world environment state.

use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::UiScenarioAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
    },
};

use super::environment_state_types::{Earthquake, WorldEnvironment};
use crate::plugins::ui::authored_ui_action_projection_components::UiScenarioActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_earthquake_ui_actions_to_world_environment_state(
    mut commands: Commands,
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    scenario_action_nodes: Query<(&UiScenarioActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    world_environments: Query<(Entity, Option<&Earthquake>), With<WorldEnvironment>>,
) {
    for activated_ui_node in activated_ui_nodes.read() {
        let Ok((authored_scenario_actions, ui_document_owner)) =
            scenario_action_nodes.get(activated_ui_node.node)
        else {
            continue;
        };
        let Ok(ui_document_root) = ui_document_roots.get(ui_document_owner.0) else {
            continue;
        };
        let Some(ui_document_asset) = ui_document_assets.get(&ui_document_root.document) else {
            continue;
        };
        for authored_scenario_action in
            authored_scenario_actions.authored_action_records(ui_document_asset)
        {
            if activated_ui_node.trigger != authored_scenario_action.trigger {
                continue;
            }
            match &authored_scenario_action.action {
                UiScenarioAction::TriggerEarthquake => {
                    for (world_environment_entity, current_earthquake) in &world_environments {
                        commands
                            .entity(world_environment_entity)
                            .insert(current_earthquake.copied().unwrap_or_default());
                    }
                }
                UiScenarioAction::RefreshEarthquake => {
                    for (world_environment_entity, current_earthquake) in &world_environments {
                        commands
                            .entity(world_environment_entity)
                            .insert(Earthquake {
                                refresh_generation: current_earthquake
                                    .map_or(0, |earthquake| earthquake.refresh_generation)
                                    .wrapping_add(1),
                            });
                    }
                }
                _ => {}
            }
        }
    }
}
