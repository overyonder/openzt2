use bevy::prelude::*;
use openzt2_interface_state::{
    information::zoo_message_types::ZooMessages,
    ui::authored_reusable_list_and_table_runtime_types::{RequestedUiListRowCount, UiListRow},
};

#[derive(Component)]
pub(super) enum UiZooMessageList {
    Latest,
    History,
}

pub(super) fn size_zoo_message_lists(
    messages: Option<Res<ZooMessages>>,
    time: Res<Time<Real>>,
    mut lists: Query<(&UiZooMessageList, &mut RequestedUiListRowCount)>,
) {
    let Some(messages) = messages else { return };
    let latest_count = u16::from(messages.latest(time.elapsed_secs_f64()).is_some());
    let history_count = u16::try_from(messages.messages.len()).unwrap_or(u16::MAX);
    for (kind, mut count) in &mut lists {
        let length = match kind {
            UiZooMessageList::Latest => latest_count,
            UiZooMessageList::History => history_count,
        };
        count.set_if_neq(RequestedUiListRowCount(Some(length)));
    }
}

pub(super) fn bind_zoo_message_rows(
    messages: Option<Res<ZooMessages>>,
    time: Res<Time<Real>>,
    lists: Query<(&UiZooMessageList, Ref<RequestedUiListRowCount>, &Children)>,
    rows: Query<Ref<UiListRow>>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    let Some(messages) = messages else { return };
    let latest = messages.latest(time.elapsed_secs_f64());
    for (kind, count, members) in &lists {
        let update_all = messages.is_changed() || count.is_changed();
        for entity in members.iter() {
            let Ok(row) = rows.get(entity) else { continue };
            if !update_all && !row.is_added() {
                continue;
            }
            let message = match kind {
                UiZooMessageList::Latest => latest,
                UiZooMessageList::History => {
                    messages.messages.iter().rev().nth(usize::from(row.index))
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
