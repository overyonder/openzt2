use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_modal_presentation::UiAuthoredModalPresentation;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationPauseRequest;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InGameOptionsOverlay {
    previous_simulation_paused_state: bool,
}

impl InGameOptionsOverlay {
    pub(super) const fn remembering_previous_pause_state(
        previous_simulation_paused_state: bool,
    ) -> Self {
        Self {
            previous_simulation_paused_state,
        }
    }
}

/// Restores the previous pause state after the options modal finishes hiding,
/// and removes the menu so the next opening starts from its authored state.
/// The menu belongs with the game view, so what it opened stays open.
pub(super) fn close_hidden_in_game_options_overlays_and_restore_simulation_pause_state(
    mut commands: Commands,
    overlays: Query<(Entity, &InGameOptionsOverlay)>,
    roots: Query<(Entity, &UiDocumentRoot)>,
    documents: Res<Assets<UiDocumentAsset>>,
    modal_nodes: Query<(&UiAuthoredModalPresentation, &UiDocumentOwner, &Visibility)>,
    mut set_paused: MessageWriter<SimulationPauseRequest>,
) {
    for (overlay_entity, overlay_state) in &overlays {
        // One options menu is open at a time; it may still be loading.
        let Some(document_root_entity) = roots.iter().find_map(|(root, document)| {
            documents
                .get(&document.document)
                .is_some_and(|asset| {
                    asset.canonical_ui_document().role == UiDocumentRole::InGameOptions
                })
                .then_some(root)
        }) else {
            continue;
        };
        if modal_nodes.iter().any(|(modal, owner, visibility)| {
            owner.0 == document_root_entity && modal.is_modal() && *visibility != Visibility::Hidden
        }) {
            continue;
        }
        set_paused.write(SimulationPauseRequest::Set(
            overlay_state.previous_simulation_paused_state,
        ));
        commands.entity(document_root_entity).despawn();
        commands.entity(overlay_entity).despawn();
    }
}

pub(super) fn close_in_game_options_overlays_when_game_exits(
    mut commands: Commands,
    overlays: Query<(Entity, &InGameOptionsOverlay)>,
    mut set_paused: MessageWriter<SimulationPauseRequest>,
) {
    for (overlay_entity, overlay_state) in &overlays {
        set_paused.write(SimulationPauseRequest::Set(
            overlay_state.previous_simulation_paused_state,
        ));
        commands.entity(overlay_entity).despawn();
    }
}
