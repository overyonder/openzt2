use bevy::prelude::*;
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{
        RequestedUiListRowCount, SetUiListRowCount, UiListPolicy, UiListRow,
    },
    ui_document_lifecycle_contracts::ShowUiDocument,
};

use super::{
    authored_document_projection::project_document_on_entity,
    document_asset_readiness::{document_dependencies_resolved, UiDocumentAssetEvents},
};
use crate::{
    authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride,
    authored_ui_layout_participation::UiAuthoredLayoutDisplay,
};

#[derive(Component)]
pub(crate) struct PendingUiListRow(u16);

pub(crate) fn record_requested_ui_list_row_counts(
    mut requests: MessageReader<SetUiListRowCount>,
    mut lists: Query<&mut RequestedUiListRowCount>,
) {
    for request in requests.read() {
        if let Ok(mut count) = lists.get_mut(request.list) {
            count.set_if_neq(RequestedUiListRowCount(Some(request.count)));
        }
    }
}

pub(crate) fn reconcile_changed_ui_list_row_counts(
    mut commands: Commands,
    lists: Query<
        (
            Entity,
            &RequestedUiListRowCount,
            Option<&Children>,
            &UiListPolicy,
        ),
        Or<(Changed<RequestedUiListRowCount>, Changed<UiListPolicy>)>,
    >,
    rows: Query<&UiListRow>,
    pending: Query<&PendingUiListRow>,
) {
    for (list, count, children, policy) in &lists {
        let Some(count) = count.0 else {
            continue;
        };
        let mut occupied = vec![false; usize::from(count)];
        for child in children.into_iter().flatten().copied() {
            let index = rows
                .get(child)
                .map(|row| row.index)
                .or_else(|_| pending.get(child).map(|row| row.0));
            if let Ok(index) = index {
                match occupied.get_mut(usize::from(index)) {
                    Some(slot) if !*slot => *slot = true,
                    _ => commands.entity(child).despawn(),
                }
            }
        }
        if policy.row_document.is_some() {
            for index in (0..count).filter(|index| !occupied[usize::from(*index)]) {
                commands.spawn((PendingUiListRow(index), ChildOf(list)));
            }
        }
    }
}

pub(crate) fn materialize_ready_ui_list_rows(
    mut commands: Commands,
    lists: Query<Ref<UiListPolicy>>,
    server: Res<AssetServer>,
    mut asset_events: UiDocumentAssetEvents,
    pending: Query<(Entity, Ref<PendingUiListRow>, &ChildOf)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
) {
    let changes = asset_events.collect_changes();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    if changes.is_empty()
        && !pending.iter().any(|(_, row, parent)| {
            row.is_added()
                || lists
                    .get(parent.parent())
                    .is_ok_and(|policy| policy.is_changed())
        })
    {
        return;
    }
    let image_selection_index = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);

    for (row, pending, parent) in &pending {
        let Ok(policy) = lists.get(parent.parent()) else {
            continue;
        };
        let Some(handle) = &policy.row_document else {
            continue;
        };
        if (!pending.is_added() && !policy.is_changed() && !changes.affects(handle.id()))
            || !document_dependencies_resolved(&server, handle)
        {
            continue;
        }
        let Some(document) = documents.get(handle) else {
            continue;
        };
        project_document_on_entity(
            &mut commands,
            &ShowUiDocument {
                document: handle.clone(),
                owner: parent.parent(),
            },
            document,
            localization,
            image_selection_index,
            false,
            Some(row),
        );
        let mut row_commands = commands.entity(row);
        match policy.source {
            UiWidgetLiveCollectionSource::ZooMessageLatest
            | UiWidgetLiveCollectionSource::ZooMessageHistory => {
                row_commands
                    .entry::<Node>()
                    .and_modify(|mut node| node.width = percent(100.0));
            }
            UiWidgetLiveCollectionSource::ScenarioObjectives => {
                row_commands.entry::<Node>().and_modify(|mut node| {
                    node.width = percent(100.0);
                    node.display = Display::Flex;
                    node.flex_direction = FlexDirection::Column;
                    node.align_items = AlignItems::Stretch;
                });
                row_commands
                    .entry::<UiAuthoredLayoutDisplay>()
                    .and_modify(|mut display| {
                        display.set_authored_display(Display::Flex);
                    });
            }
            _ => {}
        }
        row_commands
            .remove::<PendingUiListRow>()
            .insert(UiListRow { index: pending.0 });
    }
}
