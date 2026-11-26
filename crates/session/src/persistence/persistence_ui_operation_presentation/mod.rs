//! Persistence-operation wait presentation, save-then-load continuation and
//! closing the save dialog once its save completes.

#[cfg(test)]
mod tests;

use crate::shell::shell_navigation_request_types::{ExitApplication, ReturnToMainMenu};
use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::presentation::UiPresentationAction, document::UiDocumentRole,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::ui_document::ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction;
use openzt2_ui::save_dialog_cancellation::SaveDialogCancellationRequests;

use super::persistence_ui_types::{
    ExitApplicationAfterSave, OpenLoadSlotCatalogueAfterWorldSnapshotSave,
    ReturnToMainMenuAfterSave, SaveDialogSnapshotSaved,
};

pub(super) fn continue_navigation_after_matching_dialog_save(
    mut commands: Commands,
    mut cancellations: SaveDialogCancellationRequests,
    mut failed_ui_documents: MessageReader<UiDocumentAssetLoadFailed>,
    mut completions: MessageReader<SaveDialogSnapshotSaved>,
    pending: Query<(
        Has<OpenLoadSlotCatalogueAfterWorldSnapshotSave>,
        Has<ReturnToMainMenuAfterSave>,
        Has<ExitApplicationAfterSave>,
    )>,
    roots: Query<&ChildOf, With<UiDocumentRoot>>,
    mut show: MessageWriter<ShowUiRole>,
    mut return_to_menu: MessageWriter<ReturnToMainMenu>,
    mut exit: MessageWriter<ExitApplication>,
) {
    let mut cancelled = cancellations.read_cancelled_lifecycle_owners();
    cancelled.extend(failed_ui_documents.read().filter_map(|failure| {
        select_bevy_asset_path_for_canonical_ui_document_role(UiDocumentRole::SaveGame)
            .is_some_and(|path| {
                failure.document.path() == Some(&bevy::asset::AssetPath::parse(&path))
            })
            .then_some(failure.owner)
    }));
    for &owner in &cancelled {
        if pending
            .get(owner)
            .is_ok_and(|(load, returning, exiting)| load || returning || exiting)
        {
            commands.entity(owner).remove::<(
                OpenLoadSlotCatalogueAfterWorldSnapshotSave,
                ReturnToMainMenuAfterSave,
                ExitApplicationAfterSave,
            )>();
        }
    }
    for completion in completions.read() {
        let dialog = completion.dialog;
        if cancelled.contains(&dialog.owner)
            || !roots
                .get(dialog.document)
                .is_ok_and(|parent| parent.parent() == dialog.owner)
        {
            continue;
        }
        let Ok((load, returning, exiting)) = pending.get(dialog.owner) else {
            continue;
        };
        if load {
            commands
                .entity(dialog.owner)
                .remove::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>();
            show.write(ShowUiRole {
                role: UiDocumentRole::SavedGames,
                owner: dialog.owner,
            });
        }
        if returning {
            commands
                .entity(dialog.owner)
                .remove::<ReturnToMainMenuAfterSave>();
            return_to_menu.write(ReturnToMainMenu);
        }
        if exiting {
            commands
                .entity(dialog.owner)
                .remove::<ExitApplicationAfterSave>();
            exit.write(ExitApplication);
        }
    }
}

pub(super) fn close_save_dialogs_after_world_snapshot_save(
    mut completions: MessageReader<SaveDialogSnapshotSaved>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<(&UiDocumentRoot, &ChildOf)>,
    mut presentation: MessageWriter<RequestUiPresentationAction>,
) {
    for completion in completions.read() {
        let dialog = completion.dialog;
        let Ok((root, parent)) = roots.get(dialog.document) else {
            continue;
        };
        if parent.parent() == dialog.owner
            && documents.get(&root.document).is_some_and(|document| {
                document.canonical_ui_document().role == UiDocumentRole::SaveGame
            })
        {
            trace!(operation = ?completion.operation, document = ?dialog.document, "closing saved zoo dialog");
            presentation.write(RequestUiPresentationAction {
                document_root: dialog.document,
                action: UiPresentationAction::HideOwningDocument,
            });
        }
    }
}

pub(super) fn present_original_saved_game_failures(
    mut commands: Commands,
    mut catalogue_failures: MessageReader<openzt2_interface_state::persistence::saved_game_source_types::OriginalSavedGameCatalogueFailed>,
    mut load_failures: MessageReader<
        openzt2_interface_state::persistence::saved_game_source_types::OriginalSavedGameLoadFailed,
    >,
    mut presented: MessageReader<
        openzt2_interface_state::ui::notification_contracts::NotificationPresented,
    >,
    mut rejected: MessageReader<
        openzt2_interface_state::ui::notification_contracts::NotificationRejected,
    >,
    owners: Query<(), With<OriginalSavedGameFailureNotification>>,
    mut notifications: MessageWriter<
        openzt2_interface_state::ui::notification_contracts::ShowNotification,
    >,
) {
    use openzt2_interface_state::ui::notification_contracts::{
        NotificationMessage, ShowNotification,
    };
    for operation in presented
        .read()
        .map(|message| message.operation)
        .chain(rejected.read().map(|message| message.operation))
    {
        if owners.contains(operation) {
            commands.entity(operation).despawn();
        }
    }
    for reason in catalogue_failures
        .read()
        .map(|failure| &failure.reason)
        .chain(load_failures.read().map(|failure| &failure.reason))
    {
        let operation = commands.spawn(OriginalSavedGameFailureNotification).id();
        notifications.write(ShowNotification {
            operation,
            message: NotificationMessage::Text(reason.clone()),
        });
    }
}

#[derive(Component)]
pub(super) struct OriginalSavedGameFailureNotification;
