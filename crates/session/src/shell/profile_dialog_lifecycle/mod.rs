use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_modal_presentation::UiAuthoredModalPresentation;

/// The profile dialog's close button, Escape and OK hide its modal layout
/// inside the dialog's shell. Remove the whole document once that layout is
/// hidden, as the in-game options do, so the main menu is left at rest and the
/// next opening starts from the authored state.
pub(super) fn close_profile_dialog_documents_once_their_dialog_hides(
    mut commands: Commands,
    roots: Query<(Entity, &UiDocumentRoot)>,
    documents: Res<Assets<UiDocumentAsset>>,
    modal_nodes: Query<(&UiAuthoredModalPresentation, &UiDocumentOwner, &Visibility)>,
) {
    for (root, document) in &roots {
        if !documents.get(&document.document).is_some_and(|asset| {
            asset.canonical_ui_document().role == UiDocumentRole::ProfileSelect
        }) {
            continue;
        }
        let mut dialogs = modal_nodes
            .iter()
            .filter(|(modal, owner, _)| owner.0 == root && modal.is_modal())
            .peekable();
        if dialogs.peek().is_some()
            && dialogs.all(|(_, _, visibility)| *visibility == Visibility::Hidden)
        {
            commands.entity(root).despawn();
        }
    }
}
