//! Persistence-operation wait presentation, save-then-load continuation and
//! closing the save dialog once its save completes.

use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::presentation::UiPresentationAction, document::UiDocumentRole,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::ui::{
    authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_presentation_action_application::RequestUiPresentationAction,
    cursor::SetWaitCursor, save_dialog_cancellation::SaveDialogCancellationRequests,
    ui_document_lifecycle_contracts::ShowUiRole,
};

use super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceOperation,
    },
    persistence_ui_types::OpenLoadSlotCatalogueAfterWorldSnapshotSave,
    save_slot_types::{
        DeleteWorldSnapshotFromSlot, LoadWorldSnapshotFromSlot, SaveWorldSnapshotToSlot,
        WorldSnapshotDeletedFromSlot, WorldSnapshotLoadedFromSlot, WorldSnapshotSavedToSlot,
    },
};

/// Shows the wait cursor until all requested save-slot operations finish.
#[allow(clippy::too_many_arguments)]
pub(super) fn project_outstanding_world_snapshot_operations_onto_ui_wait_cursor(
    mut save_world_snapshot_requests: MessageReader<SaveWorldSnapshotToSlot>,
    mut load_world_snapshot_requests: MessageReader<LoadWorldSnapshotFromSlot>,
    mut delete_world_snapshot_requests: MessageReader<DeleteWorldSnapshotFromSlot>,
    mut save_world_snapshot_completions: MessageReader<WorldSnapshotSavedToSlot>,
    mut load_world_snapshot_completions: MessageReader<WorldSnapshotLoadedFromSlot>,
    mut delete_world_snapshot_completions: MessageReader<WorldSnapshotDeletedFromSlot>,
    mut world_snapshot_persistence_failures: MessageReader<WorldSnapshotPersistenceFailed>,
    mut outstanding_world_snapshot_operation_count: Local<u32>,
    mut wait_cursor_updates: MessageWriter<SetWaitCursor>,
) {
    let started_world_snapshot_operation_count = save_world_snapshot_requests.read().count()
        + load_world_snapshot_requests.read().count()
        + delete_world_snapshot_requests.read().count();
    let finished_world_snapshot_operation_count = save_world_snapshot_completions.read().count()
        + load_world_snapshot_completions.read().count()
        + delete_world_snapshot_completions.read().count()
        + world_snapshot_persistence_failures.read().count();
    let wait_cursor_was_active = *outstanding_world_snapshot_operation_count != 0;
    *outstanding_world_snapshot_operation_count = outstanding_world_snapshot_operation_count
        .saturating_add(u32::try_from(started_world_snapshot_operation_count).unwrap_or(u32::MAX))
        .saturating_sub(u32::try_from(finished_world_snapshot_operation_count).unwrap_or(u32::MAX));
    let wait_cursor_is_active = *outstanding_world_snapshot_operation_count != 0;
    if wait_cursor_was_active != wait_cursor_is_active {
        wait_cursor_updates.write(SetWaitCursor(wait_cursor_is_active));
    }
}

/// Resolves the one asynchronous edge represented by `ZT_LOAD_AFTER_SAVE`.
/// Save execution remains wholly owned by the persistence systems; this system
/// only changes presentation after their typed completion message arrives.
pub(super) fn continue_to_load_slot_catalogue_after_world_snapshot_save(
    mut commands: Commands,
    mut cancellations: SaveDialogCancellationRequests,
    mut saved_world_snapshot_messages: MessageReader<WorldSnapshotSavedToSlot>,
    mut world_snapshot_persistence_failures: MessageReader<WorldSnapshotPersistenceFailed>,
    pending_load_slot_catalogue_owners: Query<
        Entity,
        With<OpenLoadSlotCatalogueAfterWorldSnapshotSave>,
    >,
    mut show_ui_document_requests: MessageWriter<ShowUiRole>,
) {
    // The continuation waits on the save dialog's owner, so cancelling that
    // dialog abandons it.
    let cancelled_owners = cancellations.read_cancelled_lifecycle_owners();
    for owner in &pending_load_slot_catalogue_owners {
        if cancelled_owners.contains(&owner) {
            commands
                .entity(owner)
                .remove::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>();
        }
    }
    if saved_world_snapshot_messages.read().next().is_some() {
        for owner in &pending_load_slot_catalogue_owners {
            commands
                .entity(owner)
                .remove::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>();
            if cancelled_owners.contains(&owner) {
                continue;
            }
            show_ui_document_requests.write(ShowUiRole {
                role: UiDocumentRole::SavedGames,
                owner,
            });
        }
    }

    if world_snapshot_persistence_failures.read().any(|failure| {
        failure.persistence_operation == WorldSnapshotPersistenceOperation::SaveToSlot
    }) {
        for owner in &pending_load_slot_catalogue_owners {
            commands
                .entity(owner)
                .remove::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>();
        }
    }
}

/// A completed save closes the save dialog, as its Cancel does. A failed save
/// leaves it open so the player can try again.
pub(super) fn close_save_dialogs_after_world_snapshot_save(
    mut saved_world_snapshot_messages: MessageReader<WorldSnapshotSavedToSlot>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<(Entity, &UiDocumentRoot)>,
    mut presentation_requests: MessageWriter<RequestUiPresentationAction>,
) {
    if saved_world_snapshot_messages.read().next().is_none() {
        return;
    }
    for (document_root, root) in &roots {
        if documents.get(&root.document).is_some_and(|document| {
            document.canonical_ui_document().role == UiDocumentRole::SaveGame
        }) {
            presentation_requests.write(RequestUiPresentationAction {
                document_root,
                action: UiPresentationAction::HideOwningDocument,
            });
        }
    }
}
