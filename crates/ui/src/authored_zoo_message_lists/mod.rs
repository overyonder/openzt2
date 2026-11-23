use bevy::prelude::*;
use openzt2_interface_state::{
    information::zoo_message_types::ZooMessages,
    ui::{
        authored_reusable_list_and_table_runtime_types::{RequestedUiListRowCount, UiListRow},
        ui_document_lifecycle_contracts::InGameUiOwner,
    },
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

#[derive(Component)]
pub(super) enum UiZooMessageList {
    Latest,
    History,
}

#[derive(Component, Default)]
pub(super) struct LatestZooMessagePresentation {
    timer: Timer,
    latest: Option<(Entity, usize)>,
}

pub(super) fn update_latest_zoo_message_presentation(
    histories: Query<(Entity, Ref<ZooMessages>), With<WorldRoot>>,
    time: Res<Time<Real>>,
    mut owners: Query<&mut LatestZooMessagePresentation, With<InGameUiOwner>>,
) {
    let Ok(mut presentation) = owners.single_mut() else {
        return;
    };
    let Ok((root, history)) = histories.single() else {
        if presentation.latest.is_some() {
            presentation.latest = None;
        }
        return;
    };
    if presentation.is_added() || history.is_changed() {
        let now = time.elapsed_secs_f64();
        let active = history
            .messages
            .last()
            .filter(|message| message.expires_at_seconds > now);
        if let Some(message) = active {
            let remaining = message.expires_at_seconds - now;
            let duration = std::time::Duration::try_from_secs_f64(remaining)
                .unwrap_or(std::time::Duration::MAX);
            presentation.timer = Timer::new(duration, TimerMode::Once);
            presentation.latest = Some((root, history.messages.len() - 1));
        } else if presentation.latest.is_some() {
            presentation.latest = None;
        }
    } else if presentation.latest.is_some()
        && presentation
            .bypass_change_detection()
            .timer
            .tick(time.delta())
            .is_finished()
    {
        presentation.latest = None;
    }
}

pub(super) fn size_zoo_message_lists(
    histories: Query<(Entity, Ref<ZooMessages>), With<WorldRoot>>,
    latest: Query<Ref<LatestZooMessagePresentation>, With<InGameUiOwner>>,
    mut lists: Query<(Ref<UiZooMessageList>, &mut RequestedUiListRowCount)>,
) {
    let Ok((root, history)) = histories.single() else {
        return;
    };
    let Ok(latest) = latest.single() else {
        return;
    };
    let latest_count = u16::from(latest.latest.is_some_and(|(owner, _)| owner == root));
    let history_count = u16::try_from(history.messages.len()).unwrap_or(u16::MAX);
    for (kind, mut count) in &mut lists {
        let length = match *kind {
            UiZooMessageList::Latest if kind.is_changed() || latest.is_changed() => latest_count,
            UiZooMessageList::History if kind.is_changed() || history.is_changed() => history_count,
            _ => continue,
        };
        count.set_if_neq(RequestedUiListRowCount(Some(length)));
    }
}

pub(super) fn bind_zoo_message_rows(
    histories: Query<(Entity, Ref<ZooMessages>), With<WorldRoot>>,
    latest: Query<Ref<LatestZooMessagePresentation>, With<InGameUiOwner>>,
    lists: Query<(&UiZooMessageList, Ref<RequestedUiListRowCount>, &Children)>,
    rows: Query<Ref<UiListRow>>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    let Ok((root, history)) = histories.single() else {
        return;
    };
    let Ok(latest) = latest.single() else {
        return;
    };
    let current = latest
        .latest
        .filter(|(owner, _)| *owner == root)
        .and_then(|(_, index)| history.messages.get(index));
    for (kind, count, members) in &lists {
        let update_all = count.is_changed()
            || match kind {
                UiZooMessageList::Latest => latest.is_changed(),
                UiZooMessageList::History => history.is_changed(),
            };
        for entity in members.iter() {
            let Ok(row) = rows.get(entity) else { continue };
            if !update_all && !row.is_added() {
                continue;
            }
            let message = match kind {
                UiZooMessageList::Latest => current,
                UiZooMessageList::History => {
                    history.messages.iter().rev().nth(usize::from(row.index))
                }
            };
            let Some(message) = message else { continue };
            for text in std::iter::once(entity).chain(children.iter_descendants::<Children>(entity))
            {
                if let Ok(mut text) = texts.get_mut(text) {
                    if text.0 != message.text {
                        text.0.clone_from(&message.text);
                    }
                }
            }
        }
    }
}
