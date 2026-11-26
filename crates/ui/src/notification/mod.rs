use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{
    localization::LocalizationFormatArgument, ui_document::document::*, AssetId,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_zoo_state::animal_behavior::behavior_birth_notice_types::BehaviorBirthNotice;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;

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

#[derive(Component)]
pub(crate) struct UiGuestThoughtList;

type UiDocumentAsset =
    openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Component)]
pub(super) struct UiGuestThoughtFragments([Handle<UiDocumentAsset>; 4]);

pub(super) fn load_authored_guest_thought_fragments(
    mut commands: Commands,
    server: Res<AssetServer>,
    lists: Query<Entity, (With<UiGuestThoughtList>, Without<UiGuestThoughtFragments>)>,
) {
    for list in &lists {
        let fragments = [
            "ui/layout/guestthoughtitembad.xml",
            "ui/layout/guestthoughtitemnormal.xml",
            "ui/layout/guestthoughtitemgood.xml",
            "ui/layout/guestthoughtitemgreat.xml",
        ]
        .map(|path| server.load(path));
        commands
            .entity(list)
            .insert(UiGuestThoughtFragments(fragments));
    }
}

#[derive(Component)]
pub(super) struct UiGuestThoughtRow(Entity, usize);

#[derive(Component)]
pub(super) struct InitialGuestThoughtText(String);

pub(super) fn initialize_authored_guest_thought_text(
    mut commands: Commands,
    mut pending: Query<(Entity, &InitialGuestThoughtText, &mut Text)>,
) {
    for (entity, initial, mut text) in &mut pending {
        text.map_unchanged(|text| &mut text.0)
            .clone_from_if_neq(initial.0.as_str());
        commands.entity(entity).remove::<InitialGuestThoughtText>();
    }
}

pub(super) fn project_selected_guest_thoughts(
    mut commands: Commands,
    lists: Query<(
        Entity,
        Ref<UiGuestThoughtFragments>,
        Ref<InheritedVisibility>,
        Option<&Children>,
    )>,
    selection: Query<
        Ref<openzt2_interface_state::information::entity_selection_types::SelectedSubject>,
        With<openzt2_interface_state::ui::ui_document_lifecycle_contracts::InGameUiOwner>,
    >,
    mut deselected: RemovedComponents<
        openzt2_interface_state::information::entity_selection_types::SelectedSubject,
    >,
    guest_children: Query<
        Ref<Children>,
        With<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
    >,
    thoughts: Query<
        Ref<openzt2_zoo_state::behavior_task_execution_types::running_actions::AuthoredViewThought>,
    >,
    names: Query<(Option<Ref<Name>>, Option<Ref<SpeciesHandle>>)>,
    (species_index, species_assets): (Res<SpeciesAssets>, Res<Assets<SpeciesAsset>>),
    rows: Query<&UiGuestThoughtRow>,
    assets: Res<Assets<UiDocumentAsset>>,
    index: Res<LocalizationPrecedenceIndex>,
    locales: Res<Assets<LocalizationAsset>>,
    children: Query<&Children>,
    mut text: Query<&mut Text>,
) {
    let removed = deselected.read().count() != 0;
    let selected = selection.single().ok();
    let guest = selected
        .as_ref()
        .and_then(|selection| guest_children.get(selection.0).ok());
    let Some(localization) = index.borrow_loaded_localization_view(&locales) else {
        return;
    };
    let history = guest
        .as_ref()
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|entity| Some((entity, thoughts.get(entity).ok()?)))
        .collect::<Vec<_>>();
    let species = species_index.get(&species_assets);

    let changed = removed
        || index.is_changed()
        || locales.is_changed()
        || species_index.is_changed()
        || species_assets.is_changed()
        || selected.as_ref().is_some_and(DetectChanges::is_changed)
        || guest.as_ref().is_some_and(DetectChanges::is_changed)
        || history.iter().any(|(_, thought)| {
            thought.is_changed()
                || thought.arguments.iter().flatten().any(|&argument| {
                    names.get(argument).is_ok_and(|(name, species)| {
                        name.is_some_and(|name| name.is_changed())
                            || species.is_some_and(|species| species.is_changed())
                    })
                })
        });
    for (list, fragments, visibility, children_of_list) in &lists {
        if !visibility.get() {
            continue;
        }
        if !changed
            && !fragments.is_added()
            && !visibility.is_changed()
            && !assets.is_changed()
            && guest.is_some()
        {
            continue;
        }
        let mut existing = children_of_list
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| Some((child, rows.get(child).ok()?)))
            .collect::<Vec<_>>();
        let mut order = Vec::new();
        for (entity, thought) in &history {
            let priority = match thought.priority {
                -1 => 0,
                1 => 2,
                2 => 3,
                _ => 1,
            };
            let handle = &fragments.0[priority];
            let Some(fragment) = assets.get(handle) else {
                continue;
            };
            let arguments = thought
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(slot, &argument)| {
                    let argument = argument?;
                    let name = names
                        .get(argument)
                        .ok()
                        .and_then(|(name, subject_species)| {
                            if slot == 4 {
                                let subject_species = species?.find(subject_species?.species)?;
                                localization.find_plain_localized_text(subject_species.name_key)
                            } else {
                                name.map(|name| name.into_inner().as_str())
                            }
                        });
                    Some(LocalizationFormatArgument::Text(name.unwrap_or_default()))
                })
                .collect::<Vec<_>>();
            let mut value = String::new();
            if localization
                .write_localized_text_with_format_arguments(
                    thought.localization_key,
                    &arguments,
                    &mut value,
                )
                .is_err()
            {
                continue;
            }
            let child = if let Some(index) = existing
                .iter()
                .position(|(_, row)| row.0 == *entity && row.1 == priority)
            {
                let (child, _) = existing.swap_remove(index);
                for node in
                    std::iter::once(child).chain(children.iter_descendants::<Children>(child))
                {
                    if let Ok(mut text) = text.get_mut(node) {
                        text.map_unchanged(|text| &mut text.0)
                            .clone_from_if_neq(value.as_str());
                        break;
                    }
                }
                child
            } else {
                let (root, nodes) = super::projection::authored_document_projection::project_document_on_entity_with_nodes(
                    &mut commands,
                    &openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiDocument { document: handle.clone(), owner: list },
                    fragment, localization, None, false, None,
                );
                commands
                    .entity(root)
                    .insert(UiGuestThoughtRow(*entity, priority));
                let text_node = fragment
                    .canonical_ui_document()
                    .nodes
                    .iter()
                    .position(|node| {
                        node.kind == openzt2_game_data::ui_document::node::UiNodeKind::Text
                    });
                if let Some(node) = text_node.and_then(|index| nodes.get(index)) {
                    commands
                        .entity(*node)
                        .insert(InitialGuestThoughtText(value));
                }
                root
            };
            order.push(child);
        }
        for (child, _) in existing {
            commands.entity(child).try_despawn();
        }
        let current = children_of_list
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|&child| rows.contains(child));
        if !order.iter().copied().eq(current) {
            commands.entity(list).insert_children(0, &order);
        }
    }
}
