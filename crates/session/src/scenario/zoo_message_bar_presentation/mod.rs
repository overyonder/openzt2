use bevy::prelude::*;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_interface_state::information::zoo_message_types::ZooMessages;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::{
    SetUiListRowCount, UiListPolicy, UiListRow,
};

/// Fills the message bar's single-message list with the newest unexpired zoo
/// message and its opened queue with every message, newest first.
#[expect(clippy::needless_pass_by_value)]
pub(super) fn present_zoo_messages_in_authored_message_bar(
    messages: Res<ZooMessages>,
    time: Res<Time<Real>>,
    lists: Query<(Entity, &UiListPolicy)>,
    rows: Query<(Entity, &UiListRow)>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
    mut counted: Local<Option<(usize, bool, usize)>>,
) {
    let now = time.elapsed_secs_f64();
    let latest = messages.latest(now);
    // Request row counts when the messages or the lists change; the list owner
    // spawns rows a frame later, so their text is refreshed every frame.
    let state = (
        messages.messages.len(),
        latest.is_some(),
        lists.iter().count(),
    );
    if *counted != Some(state) {
        *counted = Some(state);
        for (list, policy) in &lists {
            let count = match policy.source {
                UiWidgetLiveCollectionSource::ZooMessageLatest => usize::from(latest.is_some()),
                UiWidgetLiveCollectionSource::ZooMessageHistory => messages.messages.len(),
                _ => continue,
            };
            row_counts.write(SetUiListRowCount {
                list,
                count: count.min(usize::from(u16::MAX)) as u16,
            });
        }
    }
    for (entity, row) in &rows {
        let Ok((_, policy)) = lists.get(row.list) else {
            continue;
        };
        let message = match policy.source {
            UiWidgetLiveCollectionSource::ZooMessageLatest => latest,
            UiWidgetLiveCollectionSource::ZooMessageHistory => {
                messages.messages.iter().rev().nth(usize::from(row.index))
            }
            _ => continue,
        };
        if let Some(message) = message {
            crate::shell::replace_text_in_list_row_and_descendants(
                entity,
                &message.text,
                &children,
                &mut texts,
            );
        }
    }
}
