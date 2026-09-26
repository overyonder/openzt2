use bevy::prelude::*;
use openzt2_game_data::ui_document::action::simulation_time::UiSimulationAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_ui_layout_participation::UiAuthoredLayoutDisplay,
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_selection_state::UiSelected,
    },
};

use super::simulation_control_types::SimulationControl;
use crate::plugins::ui::authored_ui_action_projection_components::UiSimulationActions;

/// A UI node shown while the simulation is paused.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiSimulationPausedVisibility;

/// Synchronizes the pause toggle before input so its image and next `on`/`off`
/// event reflect changes made by keyboard.
pub(super) fn project_simulation_pause_state_into_authored_pause_toggle_selection(
    simulation_control: Res<SimulationControl>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    ui_document_roots: Query<&UiDocumentRoot>,
    mut simulation_action_nodes: Query<(&UiSimulationActions, &UiDocumentOwner, &mut UiSelected)>,
) {
    for (authored_action_range, document_owner, mut selected) in &mut simulation_action_nodes {
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        let is_pause_toggle = authored_action_range
            .authored_action_records(ui_document)
            .any(|authored_action_record| {
                matches!(
                    &authored_action_record.action,
                    UiSimulationAction::SetSimulationPaused { .. }
                        | UiSimulationAction::ToggleSimulationPaused
                )
            });
        if is_pause_toggle && selected.0 != simulation_control.paused {
            selected.0 = simulation_control.paused;
        }
    }
}

/// Shows or hides pause indicators.
pub(super) fn project_simulation_pause_state_into_authored_paused_node_visibility(
    simulation_control: Res<SimulationControl>,
    mut paused_visibility_nodes: Query<
        (&UiAuthoredLayoutDisplay, &mut Node, &mut Visibility),
        With<UiSimulationPausedVisibility>,
    >,
) {
    for (authored_display, mut node, mut visibility) in &mut paused_visibility_nodes {
        let next_visibility = if simulation_control.paused {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let next_display = if simulation_control.paused {
            authored_display.authored_display()
        } else {
            Display::None
        };
        if *visibility != next_visibility {
            *visibility = next_visibility;
        }
        if node.display != next_display {
            node.display = next_display;
        }
    }
}
