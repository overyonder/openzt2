use bevy::prelude::*;
use openzt2_interface_state::ui::{
    authored_ui_node_projection_components::UiDocumentRoot,
    ui_document_asset_load_failure::UiDocumentAssetLoadFailed,
};
use openzt2_ui::authored_modal_presentation::UiAuthoredModalPresentation;

#[derive(Component)]
pub(super) struct InGameOptionsOverlay;

/// The overlay's options have been on screen. A newly loaded document can
/// still be hidden for its first frames, so only a shown overlay can finish.
#[derive(Component)]
pub(super) struct InGameOptionsOverlayShown;

pub(super) fn close_finished_in_game_options_overlays(
    mut commands: Commands,
    overlays: Query<
        (Entity, Option<&Children>, Has<InGameOptionsOverlayShown>),
        With<InGameOptionsOverlay>,
    >,
    roots: Query<&Visibility, With<UiDocumentRoot>>,
    hierarchy: Query<&Children>,
    modals: Query<(&UiAuthoredModalPresentation, &Visibility)>,
    mut failed_documents: MessageReader<UiDocumentAssetLoadFailed>,
) {
    for failure in failed_documents.read() {
        if overlays.contains(failure.owner) {
            commands.entity(failure.owner).despawn();
        }
    }
    for (overlay, children, was_shown) in &overlays {
        let Some(root) = children
            .into_iter()
            .flatten()
            .copied()
            .find(|child| roots.contains(*child))
        else {
            continue;
        };
        let shown = roots
            .get(root)
            .is_ok_and(|visibility| *visibility != Visibility::Hidden)
            && std::iter::once(root)
                .chain(hierarchy.iter_descendants(root))
                .any(|node| {
                    modals.get(node).is_ok_and(|(modal, visibility)| {
                        modal.is_modal() && *visibility != Visibility::Hidden
                    })
                });
        if shown && !was_shown {
            commands.entity(overlay).insert(InGameOptionsOverlayShown);
        } else if !shown && was_shown {
            commands.entity(overlay).despawn();
        }
    }
}
