use bevy::prelude::*;
use openzt2_game_data::ui_document::action::simulation_time::UiSimulationAction;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::simulation_control_types::{
    AuthoredUiTemporarySimulationPause, SimulationPauseRequest, SimulationPaused,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiSimulationActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Applies pause input to the simulation controls.
pub(super) fn route_pause_game_action_to_simulation_control_requests(
    mut requested_game_actions: MessageReader<ActionRequest>,
    roots: Query<&SimulationPaused, With<WorldRoot>>,
    mut requested_pause_changes: MessageWriter<SimulationPauseRequest>,
) {
    if roots.single().is_err() {
        return;
    }
    for requested_game_action in requested_game_actions.read() {
        if requested_game_action.action == GameAction::Pause {
            requested_pause_changes.write(SimulationPauseRequest::ToggleRequested);
        }
    }
}

pub(super) fn route_authored_ui_simulation_actions_to_simulation_control_requests(
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    mut game_actions: MessageReader<ActionRequest>,
    hotkey_input: openzt2_ui::authored_hotkey_keyboard_activation::AuthoredHotkeyGameplayInputOverlap,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    simulation_action_nodes: Query<(&UiSimulationActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    mut temporary_holds: Query<&mut AuthoredUiTemporarySimulationPause>,
    roots: Query<&SimulationPaused, With<WorldRoot>>,
    mut requested_pause_changes: MessageWriter<SimulationPauseRequest>,
) {
    let simulation_control = roots.single().ok();
    let keyboard_pause_was_translated = game_actions.read().fold(false, |translated, request| {
        translated
            || (request.action == GameAction::Pause
                && request.source == ActionSource::KeyboardMouse)
    });
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
                    // The pause toggle follows the simulation and reruns these
                    // events. An echo during a temporary hold, such as the Zoopedia's,
                    // would otherwise become the state that hold restores.
                    if simulation_control.map(|control| control.0) != Some(*paused) {
                        requested_pause_changes.write(SimulationPauseRequest::Set(*paused));
                    }
                }
                UiSimulationAction::SetTemporarySimulationPauseHeld { held } => {
                    if simulation_control.is_some() {
                        if let Ok(mut claim) = temporary_holds.get_mut(activated_ui_node.node) {
                            claim.set_if_neq(AuthoredUiTemporarySimulationPause(*held));
                        }
                    }
                }
                UiSimulationAction::ToggleSimulationPaused => {
                    // The same physical pause chord also has an authored HUD
                    // receiver. Preserve other authored toggles as independent requests.
                    let translated_echo = keyboard_pause_was_translated
                        && activated_ui_node.source == ActionSource::KeyboardMouse
                        && hotkey_input.uses_just_translated_game_action_chord(
                            activated_ui_node.node,
                            GameAction::Pause,
                        );
                    if simulation_control.is_some() && !translated_echo {
                        requested_pause_changes.write(SimulationPauseRequest::ToggleRequested);
                    }
                }
            }
        }
    }
}
