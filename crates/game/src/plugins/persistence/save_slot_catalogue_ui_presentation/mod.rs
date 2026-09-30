//! Saved-game list rows.

use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::UiTrigger, widget_live_collection::UiWidgetLiveCollectionSource,
};

use crate::plugins::ui::{
    authored_reusable_list_and_table_runtime_types::{SetUiListRowCount, UiListPolicy, UiListRow},
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::{UiDocumentOwner, UiValue},
    authored_ui_selection_state::UiSelected,
};

use super::save_slot_types::{
    LoadWorldSnapshotFromSlot, SaveSlotCatalogue, SaveSlotCatalogueReady, SaveSlotId,
};

pub(super) fn request_save_slot_catalogue_list_row_count_updates(
    mut save_slot_catalogue_ready_messages: MessageReader<SaveSlotCatalogueReady>,
    save_slot_catalogue: Res<SaveSlotCatalogue>,
    ui_list_nodes: Query<(Entity, Ref<UiListPolicy>)>,
    mut ui_list_row_count_updates: MessageWriter<SetUiListRowCount>,
) {
    let save_slot_catalogue_became_ready =
        save_slot_catalogue_ready_messages.read().next().is_some();
    for (ui_list_entity, ui_list_policy) in &ui_list_nodes {
        let presents_saved_games =
            ui_list_policy.source == UiWidgetLiveCollectionSource::SavedGameSlots;
        if presents_saved_games && (save_slot_catalogue_became_ready || ui_list_policy.is_added()) {
            ui_list_row_count_updates.write(SetUiListRowCount {
                list: ui_list_entity,
                count: save_slot_catalogue
                    .save_slot_records
                    .len()
                    .min(u16::MAX as usize) as u16,
            });
        }
    }
}

pub(super) fn project_save_slot_catalogue_records_onto_saved_game_list_rows(
    mut commands: Commands,
    save_slot_catalogue: Res<SaveSlotCatalogue>,
    ui_list_rows: Query<(Entity, Ref<UiListRow>, Option<&UiValue>)>,
    ui_lists: Query<&UiListPolicy>,
    ui_node_children: Query<&Children>,
    mut ui_text_nodes: Query<&mut Text>,
) {
    for (ui_list_row_entity, ui_list_row, current_value) in &ui_list_rows {
        if !save_slot_catalogue.is_changed() && !ui_list_row.is_added() && !ui_list_row.is_changed()
        {
            continue;
        }
        if !ui_lists
            .get(ui_list_row.list)
            .is_ok_and(|policy| policy.source == UiWidgetLiveCollectionSource::SavedGameSlots)
        {
            continue;
        }
        let Some(save_slot_record) = save_slot_catalogue
            .save_slot_records
            .get(usize::from(ui_list_row.index))
        else {
            commands.entity(ui_list_row_entity).remove::<UiValue>();
            commands
                .entity(ui_list_row_entity)
                .insert(Visibility::Hidden);
            continue;
        };
        // A refreshed catalogue can put a different save at this row index.
        if current_value
            .is_none_or(|value| value.0 != i64::from(save_slot_record.save_slot_identifier.0))
        {
            commands
                .entity(ui_list_row_entity)
                .insert(UiSelected(false));
        }
        commands.entity(ui_list_row_entity).insert((
            UiValue(i64::from(save_slot_record.save_slot_identifier.0)),
            Visibility::Inherited,
        ));
        let saved_at = i64::try_from(save_slot_record.last_saved_unix_timestamp_milliseconds)
            .ok()
            .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
            .map(|timestamp| timestamp.format("%Y-%m-%d %H:%M UTC").to_string());
        let save_slot_row_label = saved_at.map_or_else(
            || save_slot_record.world_display_name.clone(),
            |saved_at| format!("{} | {saved_at}", save_slot_record.world_display_name),
        );
        apply_save_slot_identifier_and_label_to_ui_row_descendants(
            &mut commands,
            ui_list_row_entity,
            save_slot_record.save_slot_identifier,
            &save_slot_row_label,
            &ui_node_children,
            &mut ui_text_nodes,
        );
    }
}

fn apply_save_slot_identifier_and_label_to_ui_row_descendants(
    commands: &mut Commands,
    ui_node_entity: Entity,
    save_slot_identifier: SaveSlotId,
    save_slot_row_label: &str,
    ui_node_children: &Query<&Children>,
    ui_text_nodes: &mut Query<&mut Text>,
) {
    if let Ok(mut text) = ui_text_nodes.get_mut(ui_node_entity) {
        if text.0 != save_slot_row_label {
            text.0 = save_slot_row_label.to_owned();
        }
    }
    let Ok(child_ui_nodes) = ui_node_children.get(ui_node_entity) else {
        return;
    };
    for child_ui_node in child_ui_nodes.iter() {
        commands
            .entity(child_ui_node)
            .insert(UiValue(i64::from(save_slot_identifier.0)));
        apply_save_slot_identifier_and_label_to_ui_row_descendants(
            commands,
            child_ui_node,
            save_slot_identifier,
            save_slot_row_label,
            ui_node_children,
            ui_text_nodes,
        );
    }
}

pub(super) fn selected_save_slot_in_document(
    document_root: Entity,
    rows: &Query<(&UiListRow, &UiSelected, &UiValue)>,
    lists: &Query<(&UiDocumentOwner, &UiListPolicy)>,
) -> Option<u32> {
    rows.iter().find_map(|(row, selected, value)| {
        (selected.0
            && lists.get(row.list).is_ok_and(|(owner, policy)| {
                owner.0 == document_root
                    && policy.source == UiWidgetLiveCollectionSource::SavedGameSlots
            }))
        .then(|| u32::try_from(value.0).ok())
        .flatten()
    })
}

pub(super) fn load_save_slot_from_submitted_row(
    mut activations: MessageReader<UiNodeActivated>,
    rows: Query<(&UiListRow, &UiValue)>,
    lists: Query<&UiListPolicy>,
    mut loads: MessageWriter<LoadWorldSnapshotFromSlot>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Submit {
            continue;
        }
        let Ok((row, value)) = rows.get(activation.node) else {
            continue;
        };
        if !lists
            .get(row.list)
            .is_ok_and(|policy| policy.source == UiWidgetLiveCollectionSource::SavedGameSlots)
        {
            continue;
        }
        if let Ok(slot) = u32::try_from(value.0) {
            loads.write(LoadWorldSnapshotFromSlot {
                save_slot_identifier: SaveSlotId(slot),
            });
        }
    }
}
