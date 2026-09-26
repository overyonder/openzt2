use bevy::prelude::*;
use openzt2_game_data::ui_document::action::simulation_time::UiSimulationAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::input::input_types::{ActionRequest, GameAction},
    plugins::ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
    },
};

use super::simulation_control_types::{SetSimulationPaused, SimulationControl};
use crate::plugins::ui::authored_ui_action_projection_components::UiSimulationActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Applies pause input to the simulation controls.
pub(super) fn route_pause_game_action_to_simulation_control_requests(
    mut requested_game_actions: MessageReader<ActionRequest>,
    simulation_control: Option<Res<SimulationControl>>,
    mut requested_pause_changes: MessageWriter<SetSimulationPaused>,
) {
    let Some(simulation_control) = simulation_control else {
        return;
    };
    let mut next_paused_state = simulation_control.paused;
    for requested_game_action in requested_game_actions.read() {
        if requested_game_action.action == GameAction::Pause {
            next_paused_state = !next_paused_state;
            requested_pause_changes.write(SetSimulationPaused(next_paused_state));
        }
    }
}

pub(super) fn route_authored_ui_simulation_actions_to_simulation_control_requests(
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    simulation_action_nodes: Query<(&UiSimulationActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    simulation_control: Option<Res<SimulationControl>>,
    mut requested_pause_changes: MessageWriter<SetSimulationPaused>,
) {
    for activated_ui_node in activated_ui_nodes.read() {
        let Ok((authored_action_range, document_owner)) =
            simulation_action_nodes.get(activated_ui_node.node)
        else {
            continue;
        };
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        for authored_action_record in authored_action_range.authored_action_records(ui_document) {
            if activated_ui_node.trigger != authored_action_record.trigger {
                continue;
            }
            match &authored_action_record.action {
                UiSimulationAction::SetSimulationPaused { paused } => {
                    requested_pause_changes.write(SetSimulationPaused(*paused));
                }
                UiSimulationAction::ToggleSimulationPaused => {
                    if let Some(simulation_control) = simulation_control.as_deref() {
                        requested_pause_changes
                            .write(SetSimulationPaused(!simulation_control.paused));
                    }
                }
            }
        }
    }
}
