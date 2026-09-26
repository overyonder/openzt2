//! Saved-game list rows.

use bevy::prelude::*;

use crate::plugins::{
    shell::shell_selection_types::ShellScreen,
    ui::{
        authored_reusable_list_and_table_runtime_types::{
            SetUiListRowCount, UiListPolicy, UiListRow,
        },
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
    },
};

use super::{
    persistence_ui_types::{
        SaveSlotCataloguePresentedForLoading, SaveSlotCataloguePresentedForSaving,
    },
    save_slot_types::{SaveSlotCatalogue, SaveSlotCatalogueReady, SaveSlotId},
};

pub(super) fn request_save_slot_catalogue_list_row_count_updates(
    mut save_slot_catalogue_ready_messages: MessageReader<SaveSlotCatalogueReady>,
    save_slot_catalogue: Res<SaveSlotCatalogue>,
    ui_list_nodes: Query<(Entity, &UiDocumentOwner, Ref<UiListPolicy>)>,
    save_slot_catalogue_document_owners: Query<
        (),
        Or<(
            With<SaveSlotCataloguePresentedForSaving>,
            With<SaveSlotCataloguePresentedForLoading>,
        )>,
    >,
    shell_screen_owners: Query<&ShellScreen>,
    mut ui_list_row_count_updates: MessageWriter<SetUiListRowCount>,
) {
    let save_slot_catalogue_became_ready =
        save_slot_catalogue_ready_messages.read().next().is_some();
    for (ui_list_entity, ui_document_owner, ui_list_policy) in &ui_list_nodes {
        let presents_saved_games = save_slot_catalogue_document_owners
            .get(ui_document_owner.0)
            .is_ok()
            || shell_screen_owners
                .get(ui_document_owner.0)
                .is_ok_and(|shell_screen| *shell_screen == ShellScreen::SavedGames);
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
    ui_list_rows: Query<(Entity, Ref<UiListRow>)>,
    ui_list_document_owners: Query<&UiDocumentOwner, With<UiListPolicy>>,
    save_slot_catalogue_document_owners: Query<
        (),
        Or<(
            With<SaveSlotCataloguePresentedForSaving>,
            With<SaveSlotCataloguePresentedForLoading>,
        )>,
    >,
    shell_screen_owners: Query<&ShellScreen>,
    ui_node_children: Query<&Children>,
    mut ui_text_nodes: Query<&mut Text>,
) {
    for (ui_list_row_entity, ui_list_row) in &ui_list_rows {
        if !save_slot_catalogue.is_changed() && !ui_list_row.is_added() && !ui_list_row.is_changed()
        {
            continue;
        }
        let Ok(ui_document_owner) = ui_list_document_owners.get(ui_list_row.list) else {
            continue;
        };
        let presents_saved_games = save_slot_catalogue_document_owners
            .get(ui_document_owner.0)
            .is_ok()
            || shell_screen_owners
                .get(ui_document_owner.0)
                .is_ok_and(|shell_screen| *shell_screen == ShellScreen::SavedGames);
        if !presents_saved_games {
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
    let Ok(child_ui_nodes) = ui_node_children.get(ui_node_entity) else {
        return;
    };
    for child_ui_node in child_ui_nodes.iter() {
        commands
            .entity(child_ui_node)
            .insert(UiValue(i64::from(save_slot_identifier.0)));
        if let Ok(mut ui_text) = ui_text_nodes.get_mut(child_ui_node) {
            if ui_text.0 != save_slot_row_label {
                ui_text.0.clear();
                ui_text.0.push_str(save_slot_row_label);
            }
        }
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
