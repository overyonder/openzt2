use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{
    localization::LocalizationFormatArgument, ui_document::document::*, AssetId,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_zoo_state::animal_behavior::behavior_birth_notice_types::BehaviorBirthNotice;

use super::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;

#[cfg(test)]
mod tests;

use super::{
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
pub(super) struct ProjectedNotificationAlert {
    pub(super) body: Entity,
    pub(super) owner: Entity,
}

#[derive(Component)]
pub(super) struct NotificationAlertShowing {
    owner: Entity,
    operation: Entity,
}

#[derive(Component)]
pub(super) struct NotificationAlertPresented;

#[derive(Component)]
struct PendingBirthNotification(String);

#[derive(SystemParam)]
pub(super) struct BirthNotifications<'w, 's> {
    notices: Option<MessageReader<'w, 's, BehaviorBirthNotice>>,
    subjects: Query<'w, 's, (Option<&'static Name>, Option<&'static WorldMember>)>,
    roots: Query<'w, 's, Entity, With<WorldRoot>>,
    pending: Query<'w, 's, (Entity, &'static ChildOf, &'static PendingBirthNotification)>,
    messages: Query<'w, 's, &'static mut ZooMessages, With<WorldRoot>>,
    time: Option<Res<'w, Time<Real>>>,
}

pub(super) fn request_notifications(
    mut commands: Commands,
    mut requests: MessageReader<ShowNotification>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    pending: Query<(Entity, &PendingNotification), Without<NotificationProjectionRequested>>,
    mut show: MessageWriter<ShowUiRole>,
    mut rejected: MessageWriter<NotificationRejected>,
    mut births: BirthNotifications,
) {
    for request in requests.read() {
        commands.spawn(PendingNotification {
            operation: request.operation,
            message: request.message.clone(),
        });
    }

    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    if let Some(notices) = &mut births.notices {
        for notice in notices.read() {
            let subject = births.subjects.get(notice.subject).ok();
            let name = notice
                .mother_name
                .as_deref()
                .or_else(|| subject.and_then(|(name, _)| name).map(Name::as_str));
            let root = subject
                .and_then(|(_, world)| world.map(|world| world.root))
                .or_else(|| births.roots.single().ok());
            if let (Some(name), Some(root)) = (name, root) {
                commands.spawn((PendingBirthNotification(name.to_owned()), ChildOf(root)));
            }
        }
    }
    if !births.pending.is_empty() {
        if let (Some(locale), Some(time)) = (localization, births.time.as_deref()) {
            let key = AssetId::from_key("zoomessages:AnimalBirth");
            for (entity, parent, notice) in &births.pending {
                let Ok(mut messages) = births.messages.get_mut(parent.parent()) else {
                    continue;
                };
                let mut text = String::new();
                if locale
                    .write_localized_text_with_format_arguments(
                        key,
                        &[LocalizationFormatArgument::Text(&notice.0)],
                        &mut text,
                    )
                    .is_err()
                {
                    continue;
                }
                messages.messages.push(ZooMessage {
                    text,
                    expires_at_seconds: time.elapsed_secs_f64() + 10.0,
                });
                commands.entity(entity).despawn();
            }
        }
    }
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

pub(super) fn populate_projected_notification_alerts(
    mut commands: Commands,
    mut unpopulated: Query<
        (Entity, Ref<ProjectedNotificationAlert>, &mut Visibility),
        Without<NotificationAlertShowing>,
    >,
    pending: Query<&PendingNotification>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut texts: Query<&mut Text>,
    animations: Query<(), With<crate::animation::UiShowHideAnimation>>,
    mut playback: MessageWriter<crate::animation::UiAnimationPlaybackRequest>,
    mut activations: MessageWriter<
        openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated,
    >,
    mut rejected: MessageWriter<NotificationRejected>,
) {
    let locale_changed = active_localization.is_changed() || localizations.is_changed();
    let locale = active_localization.borrow_loaded_localization_view(&localizations);
    for (alert, projection, mut visibility) in &mut unpopulated {
        if !projection.is_added() && !locale_changed {
            continue;
        }
        let Ok(notification) = pending.get(projection.owner) else {
            continue;
        };
        let text = match &notification.message {
            NotificationMessage::Text(text) => text.as_str(),
            NotificationMessage::Localized(key) => {
                let Some(locale) = locale else { continue };
                let Some(text) = localized(locale, *key) else {
                    rejected.write(NotificationRejected {
                        operation: notification.operation,
                        reason: NotificationFailure::MissingLocalization(*key),
                    });
                    commands.entity(projection.owner).try_despawn();
                    continue;
                };
                text
            }
        };
        let Ok(body) = texts.get_mut(projection.body) else {
            rejected.write(NotificationRejected {
                operation: notification.operation,
                reason: NotificationFailure::ProjectionFailed,
            });
            commands.entity(projection.owner).try_despawn();
            continue;
        };
        body.map_unchanged(|body| &mut body.0)
            .clone_from_if_neq(text);
        if animations.contains(alert) {
            playback.write(crate::animation::UiAnimationPlaybackRequest {
                entity: alert,
                operation: crate::animation::UiAnimationPlaybackOperation::Transition(true),
            });
        }
        crate::authored_ui_change_activation_dispatch::apply_visibility_transition(
            alert,
            &mut visibility,
            Visibility::Inherited,
            true,
            &mut activations,
        );
        commands.entity(alert).insert(NotificationAlertShowing {
            owner: projection.owner,
            operation: notification.operation,
        });
    }
}

pub(super) fn report_effectively_visible_notifications(
    mut commands: Commands,
    alerts: Query<
        (
            Entity,
            &NotificationAlertShowing,
            &InheritedVisibility,
            Has<NotificationAlertPresented>,
        ),
        Or<(
            Added<NotificationAlertShowing>,
            Changed<InheritedVisibility>,
        )>,
    >,
    mut presented: MessageWriter<NotificationPresented>,
) {
    for (alert, notification, visibility, was_presented) in &alerts {
        match (visibility.get(), was_presented) {
            (true, false) => {
                presented.write(NotificationPresented {
                    operation: notification.operation,
                });
                commands.entity(alert).insert(NotificationAlertPresented);
                commands
                    .entity(notification.owner)
                    .remove::<PendingNotification>();
            }
            (false, true) => commands.entity(notification.owner).try_despawn(),
            _ => {}
        }
    }
}

pub(super) fn discard_notification_without_projected_alert(
    removal: On<Remove<NotificationAlertShowing>>,
    mut commands: Commands,
    alerts: Query<&NotificationAlertShowing>,
) {
    if let Ok(alert) = alerts.get(removal.entity) {
        commands.entity(alert.owner).try_despawn();
    }
}
