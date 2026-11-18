use bevy::prelude::*;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerInteraction;

use super::authored_ui_focus_state::UiFocusable;
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use super::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SetAuthoredUiNodeInteractionEnabled {
    document_root: Entity,
    node_index: u32,
    enabled: bool,
}

impl SetAuthoredUiNodeInteractionEnabled {
    pub(super) fn for_projected_node(
        document_root: Entity,
        node_index: u32,
        enabled: bool,
    ) -> Self {
        Self {
            document_root,
            node_index,
            enabled,
        }
    }
}

pub(super) fn apply_authored_ui_node_interaction_enabled_state_changes(
    mut changes: MessageReader<SetAuthoredUiNodeInteractionEnabled>,
    mut nodes: Query<(
        &UiNodeId,
        &UiDocumentOwner,
        &mut UiInteractionEnabled,
        Option<&mut UiFocusable>,
        &mut UiPointerInteraction,
    )>,
) {
    for change in changes.read() {
        for (node_id, document_owner, mut enabled, focusable, mut interaction) in &mut nodes {
            if document_owner.0 == change.document_root && node_id.index == change.node_index {
                enabled.0 = change.enabled;
                if let Some(mut focusable) = focusable {
                    focusable.enabled = change.enabled;
                }
                if !change.enabled {
                    *interaction = UiPointerInteraction::None;
                }
                break;
            }
        }
    }
}
