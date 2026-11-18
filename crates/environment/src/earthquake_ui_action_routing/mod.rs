//! Authored earthquake UI actions applied directly to world environment state.

use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::UiScenarioAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use openzt2_interface_state::ui::authored_ui_action_projection_components::UiScenarioActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

use super::environment_state_types::{Earthquake, EarthquakeRefreshed};

pub(super) fn route_authored_earthquake_ui_actions_to_world_environment_state(
    mut commands: Commands,
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    scenario_action_nodes: Query<(&UiScenarioActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    world_environments: Query<Entity, With<WorldEnvironment>>,
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
            let refresh = match &authored_scenario_action.action {
                UiScenarioAction::TriggerEarthquake => false,
                UiScenarioAction::RefreshEarthquake => true,
                _ => continue,
            };
            for entity in &world_environments {
                commands.entity(entity).insert_if_new(Earthquake);
                if refresh {
                    commands.trigger(EarthquakeRefreshed { entity });
                }
            }
        }
    }
}
