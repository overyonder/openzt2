use bevy::prelude::*;
use openzt2_game_data::ui_document::document::*;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;

use super::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;

#[cfg(test)]
mod tests;

use super::{
    authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId},
    localized_ui_text_writing::localized_ui_text as localized,
    notification_contracts::{
        NotificationFailure, NotificationMessage, NotificationPresented, NotificationRejected,
        PendingNotification, ShowNotification,
    },
    ui_document_lifecycle_contracts::ShowUiRole,
};

#[derive(Component)]
pub(super) struct NotificationProjectionRequested;

#[derive(Component)]
pub(super) struct NotificationBodyRequested;

#[derive(Component)]
pub(super) struct PresentedNotification;

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
            message: request.message.clone(),
        });
    }

    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    // Every request becomes a durable ECS owner before localization or the
    // modal document is consulted. Asset latency therefore cannot make a
    // one-frame gameplay notification disappear.
    for (owner, notification) in &pending {
        if let NotificationMessage::Localized(key) = &notification.message {
            let Some(localization) = localization else {
                continue;
            };
            if localized(localization, *key).is_none() {
                rejected.write(NotificationRejected {
                    operation: notification.operation,
                    reason: NotificationFailure::MissingLocalization(*key),
                });
                commands.entity(owner).despawn();
                continue;
            }
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
    roots: Query<(Entity, &ChildOf), With<UiDocumentRoot>>,
    pending: Query<
        (
            Entity,
            Option<&PendingNotification>,
            Has<NotificationBodyRequested>,
            Has<PresentedNotification>,
        ),
        Or<(With<PendingNotification>, With<PresentedNotification>)>,
    >,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    nodes: Query<(
        &UiNodeId,
        &UiDocumentOwner,
        &InheritedVisibility,
        Option<&Text>,
    )>,
    mut presentation: MessageWriter<
        super::authored_ui_presentation_action_application::RequestUiPresentationAction,
    >,
    mut presented: MessageWriter<NotificationPresented>,
) {
    use super::authored_ui_presentation_action_application::RequestUiPresentationAction;
    use openzt2_game_data::ui_document::action::presentation::UiPresentationAction;
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let alert = UiDocumentRole::Modal.node_id("Alert Box");
    let body = UiDocumentRole::Modal.node_id("Alert Text");
    for (owner, notification, requested, was_presented) in &pending {
        let root = roots
            .iter()
            .find_map(|(root, parent)| (parent.parent() == owner).then_some(root));
        if was_presented {
            if root.is_none_or(|root| {
                nodes.iter().any(|(id, node_owner, visibility, _)| {
                    node_owner.0 == root && id.id == alert && !visibility.get()
                })
            }) {
                commands.entity(owner).despawn();
            }
            continue;
        }
        let (Some(root), Some(notification)) = (root, notification) else {
            continue;
        };
        let text = match &notification.message {
            NotificationMessage::Text(text) => text.as_str(),
            NotificationMessage::Localized(key) => {
                let Some(text) = localization.and_then(|view| localized(view, *key)) else {
                    continue;
                };
                text
            }
        };
        if !requested {
            if !nodes.iter().any(|(id, node_owner, _, value)| {
                node_owner.0 == root && id.id == body && value.is_some()
            }) {
                continue;
            }
            for action in [
                UiPresentationAction::SetTargetNodeTextLiteral {
                    target_node: body,
                    text: text.to_owned(),
                },
                UiPresentationAction::SetTargetNodeVisible {
                    target_node: alert,
                    visible: true,
                },
            ] {
                presentation.write(RequestUiPresentationAction {
                    document_root: root,
                    action,
                });
            }
            commands.entity(owner).insert(NotificationBodyRequested);
            continue;
        }
        let visible = nodes.iter().any(|(id, node_owner, visibility, _)| {
            node_owner.0 == root && id.id == alert && visibility.get()
        });
        let populated = nodes.iter().any(|(id, node_owner, _, value)| {
            node_owner.0 == root && id.id == body && value.is_some_and(|value| value.0 == text)
        });
        if !visible || !populated {
            continue;
        }
        presented.write(NotificationPresented {
            operation: notification.operation,
        });
        commands
            .entity(owner)
            .insert(PresentedNotification)
            .remove::<(PendingNotification, NotificationBodyRequested)>();
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
        commands.entity(failure.owner).despawn();
    }
}
