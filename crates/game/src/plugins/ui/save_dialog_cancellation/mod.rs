use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::{
    action::presentation::UiPresentationAction, document::UiDocumentRole,
};

use super::{
    authored_ui_action_projection_components::UiPresentationActions,
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
};
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(SystemParam)]
pub(crate) struct SaveDialogCancellationRequests<'w, 's> {
    activations: MessageReader<'w, 's, UiNodeActivated>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    nodes: Query<'w, 's, (&'static UiPresentationActions, &'static UiDocumentOwner)>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, &'static ChildOf)>,
}

impl SaveDialogCancellationRequests<'_, '_> {
    pub(crate) fn read_cancelled_lifecycle_owners(&mut self) -> Vec<Entity> {
        self.activations
            .read()
            .filter_map(|activation| {
                let (actions, owner) = self.nodes.get(activation.node).ok()?;
                let (root, lifecycle_owner) = self.roots.get(owner.0).ok()?;
                let document = self.documents.get(&root.document)?;
                (document.canonical_ui_document().role == UiDocumentRole::SaveGame
                    && actions.authored_action_records(document).any(|record| {
                        record.trigger == activation.trigger
                            && record.action == UiPresentationAction::HideOwningDocument
                    }))
                .then_some(lifecycle_owner.parent())
            })
            .collect()
    }
}
