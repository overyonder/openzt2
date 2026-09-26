use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        simulation_time::simulation_control_types::SetSimulationPaused,
        ui::{
            authored_modal_presentation::UiAuthoredModalPresentation,
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};

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

/// Restores the previous pause state after the options modal finishes hiding.
pub(super) fn close_hidden_in_game_options_overlays_and_restore_simulation_pause_state(
    mut commands: Commands,
    overlays: Query<(Entity, &InGameOptionsOverlay)>,
    roots: Query<(Entity, &ChildOf, &UiDocumentRoot)>,
    documents: Res<Assets<UiDocumentAsset>>,
    modal_nodes: Query<(&UiAuthoredModalPresentation, &UiDocumentOwner, &Visibility)>,
    mut set_paused: MessageWriter<SetSimulationPaused>,
) {
    for (overlay_entity, overlay_state) in &overlays {
        let Some(document_root_entity) = roots.iter().find_map(|(root, parent, document)| {
            (parent.parent() == overlay_entity
                && documents.get(&document.document).is_some_and(|asset| {
                    matches!(
                        &asset.canonical_ui_document().role,
                        UiDocumentRole::InGameOptions
                    )
                }))
            .then_some(root)
        }) else {
            continue;
        };
        if modal_nodes.iter().any(|(modal, owner, visibility)| {
            owner.0 == document_root_entity && modal.is_modal() && *visibility != Visibility::Hidden
        }) {
            continue;
        }
        set_paused.write(SetSimulationPaused(
            overlay_state.previous_simulation_paused_state,
        ));
        commands.entity(overlay_entity).despawn();
    }
}

pub(super) fn close_in_game_options_overlays_when_game_exits(
    mut commands: Commands,
    overlays: Query<(Entity, &InGameOptionsOverlay)>,
    mut set_paused: MessageWriter<SetSimulationPaused>,
) {
    for (overlay_entity, overlay_state) in &overlays {
        set_paused.write(SetSimulationPaused(
            overlay_state.previous_simulation_paused_state,
        ));
        commands.entity(overlay_entity).despawn();
    }
}
