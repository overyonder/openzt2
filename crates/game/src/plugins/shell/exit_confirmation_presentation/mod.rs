use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiShellPresentationSlotBinding;

use crate::plugins::ui::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_presentation_slot_bindings::UiShellPresentationBinding,
};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ExitConfirmationPending;

/// Reveals the authored confirmation leaf after its modal document appears.
pub(super) fn reveal_authored_exit_confirmation_after_modal_projection(
    roots: Query<(Entity, &ChildOf), Added<UiDocumentRoot>>,
    pending: Query<(), With<ExitConfirmationPending>>,
    mut nodes: Query<(
        &UiShellPresentationBinding,
        &UiDocumentOwner,
        &mut Visibility,
    )>,
) {
    for (root_entity, parent) in &roots {
        if pending.get(parent.parent()).is_err() {
            continue;
        }
        for (binding, owner, mut visibility) in &mut nodes {
            if owner.0 == root_entity
                && binding.0 == UiShellPresentationSlotBinding::ExitConfirmation
            {
                *visibility = Visibility::Inherited;
            }
        }
    }
}
