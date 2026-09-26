use bevy::prelude::*;
use openzt2_game_data::ui_document::document::*;

use crate::assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};

use super::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;
use super::{
    authored_ui_node_projection_components::UiDocumentRoot,
    localized_ui_text_writing::localized_ui_text as localized,
    notification_contracts::{
        NotificationFailure, NotificationPresented, NotificationRejected, PendingNotification,
        ShowNotification,
    },
    ui_document_lifecycle_contracts::ShowUiRole,
};

#[derive(Component)]
pub(super) struct NotificationProjectionRequested;

pub(super) fn request_notifications(
    mut commands: Commands,
    mut requests: MessageReader<ShowNotification>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    pending: Query<(Entity, &PendingNotification), Without<NotificationProjectionRequested>>,
    mut show: MessageWriter<ShowUiRole>,
    mut rejected: MessageWriter<NotificationRejected>,
) {
    for request in requests.read() {
        commands.spawn(PendingNotification {
            operation: request.operation,
            message: request.message,
        });
    }

    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    // Every request becomes a durable ECS owner before localization or the
    // modal document is consulted. Asset latency therefore cannot make a
    // one-frame gameplay notification disappear.
    for (owner, notification) in &pending {
        if localized(localization, notification.message).is_none() {
            rejected.write(NotificationRejected {
                operation: notification.operation,
                reason: NotificationFailure::MissingLocalization(notification.message),
            });
            commands.entity(owner).despawn();
            continue;
        }
        show.write(ShowUiRole {
            role: UiDocumentRole::Modal,
            owner,
        });
        commands
            .entity(owner)
            .insert(NotificationProjectionRequested);
    }
}

pub(super) fn acknowledge_notifications(
    mut commands: Commands,
    roots: Query<&ChildOf, Added<UiDocumentRoot>>,
    pending: Query<&PendingNotification>,
    mut presented: MessageWriter<NotificationPresented>,
) {
    for parent in &roots {
        let owner = parent.parent();
        let Ok(notification) = pending.get(owner) else {
            continue;
        };
        presented.write(NotificationPresented {
            operation: notification.operation,
        });
        commands.entity(owner).remove::<PendingNotification>();
    }
}

pub(super) fn reject_failed_notifications(
    mut commands: Commands,
    mut failures: MessageReader<UiDocumentAssetLoadFailed>,
    pending: Query<&PendingNotification>,
    mut rejected: MessageWriter<NotificationRejected>,
) {
    for failure in failures.read() {
        let Ok(notification) = pending.get(failure.owner) else {
            continue;
        };
        rejected.write(NotificationRejected {
            operation: notification.operation,
            reason: NotificationFailure::ProjectionFailed,
        });
        commands
            .entity(failure.owner)
            .remove::<PendingNotification>();
    }
}
