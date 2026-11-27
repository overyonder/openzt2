//! Saved-game list rows.

use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::{
    action::UiTrigger, widget_live_collection::UiWidgetLiveCollectionSource,
};
use openzt2_interface_state::{
    persistence::saved_game_source_types::{LoadOriginalSavedGame, SavedGameSource},
    ui::authored_ui_node_projection_components::UiDocumentRoot,
};

use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;

use openzt2_interface_state::persistence::save_slot_types::LoadWorldSnapshotFromSlot;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogue;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogueReady;

pub(super) fn request_save_slot_catalogue_list_row_count_updates(
    mut save_slot_catalogue_ready_messages: MessageReader<SaveSlotCatalogueReady>,
    save_slot_catalogue: Res<SaveSlotCatalogue>,
    ui_list_nodes: Query<(
        Entity,
        Ref<UiListPolicy>,
        &UiDocumentOwner,
        &openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::RequestedUiListRowCount,
    )>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut ui_list_row_count_updates: MessageWriter<SetUiListRowCount>,
) {
    let save_slot_catalogue_became_ready =
        save_slot_catalogue_ready_messages.read().next().is_some();
    for (ui_list_entity, ui_list_policy, owner, requested) in &ui_list_nodes {
        let presents_saved_games =
            ui_list_policy.source == UiWidgetLiveCollectionSource::SavedGameSlots;
        // A list whose document opens without a catalogue reload, such as the
        // Save dialog, still needs its first row count.
        if presents_saved_games
            && (save_slot_catalogue_became_ready
                || ui_list_policy.is_added()
                || requested.0.is_none())
        {
            ui_list_row_count_updates.write(SetUiListRowCount {
                list: ui_list_entity,
                count: save_slot_catalogue
                    .save_slot_records
                    .len()
                    .saturating_add(if presents_originals(owner.0, &roots, &documents) {
                        save_slot_catalogue.original_saved_game_records.len()
                    } else {
                        0
                    })
                    .min(u16::MAX as usize) as u16,
            });
        }
    }
}

pub(super) fn project_save_slot_catalogue_records_onto_saved_game_list_rows(
    mut commands: Commands,
    save_slot_catalogue: Res<SaveSlotCatalogue>,
    mut ui_list_rows: Query<(
        Entity,
        Ref<UiListRow>,
        Ref<ChildOf>,
        Option<&mut SavedGameSource>,
        Option<&mut Visibility>,
        Option<&mut UiSelected>,
        Option<&mut UiValue>,
    )>,
    ui_lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    ui_node_children: Query<&Children>,
    mut ui_text_nodes: Query<&mut Text>,
) {
    for (entity, row, parent, current, visibility, selected, value) in &mut ui_list_rows {
        if !save_slot_catalogue.is_changed() && !row.is_changed() && !parent.is_changed() {
            continue;
        }
        if !ui_lists
            .get(parent.parent())
            .is_ok_and(|(policy, _)| policy.source == UiWidgetLiveCollectionSource::SavedGameSlots)
        {
            continue;
        }
        let index = usize::from(row.index);
        let record = save_slot_catalogue
            .save_slot_records
            .get(index)
            .map(|record| {
                (
                    SavedGameSource::Native(record.save_slot_identifier),
                    record.world_display_name.clone(),
                    record.last_saved_unix_timestamp_milliseconds,
                )
            })
            .or_else(|| {
                let (_, owner) = ui_lists.get(parent.parent()).ok()?;
                if !presents_originals(owner.0, &roots, &documents) {
                    return None;
                }
                let original = save_slot_catalogue
                    .original_saved_game_records
                    .get(index.checked_sub(save_slot_catalogue.save_slot_records.len())?)?;
                Some((
                    SavedGameSource::Original(original.source_identifier),
                    format!(
                        "{} | {} (read only)",
                        original.world_display_name, original.profile_display_name
                    ),
                    original.last_saved_unix_timestamp_milliseconds,
                ))
            });
        let visible = if record.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(mut visibility) = visibility {
            visibility.set_if_neq(visible);
        } else {
            commands.entity(entity).insert(visible);
        }
        let Some((source, name, timestamp)) = record else {
            commands
                .entity(entity)
                .remove::<(SavedGameSource, UiValue)>();
            continue;
        };
        let rebound = if let Some(mut current) = current {
            current.set_if_neq(source)
        } else {
            commands.entity(entity).insert(source);
            true
        };
        if rebound {
            if let Some(mut selected) = selected {
                selected.set_if_neq(UiSelected(false));
            } else {
                commands.entity(entity).insert(UiSelected(false));
            }
        }
        match (source, value) {
            (SavedGameSource::Native(slot), Some(mut value)) => {
                value.set_if_neq(UiValue(i64::from(slot.0)));
            }
            (SavedGameSource::Native(slot), None) => {
                commands.entity(entity).insert(UiValue(i64::from(slot.0)));
            }
            (SavedGameSource::Original(_), Some(_)) => {
                commands.entity(entity).remove::<UiValue>();
            }
            (SavedGameSource::Original(_), None) => {}
        }
        let saved_at = i64::try_from(timestamp)
            .ok()
            .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
            .map(|timestamp| timestamp.format("%Y-%m-%d %H:%M UTC").to_string());
        let save_slot_row_label =
            saved_at.map_or_else(|| name.clone(), |saved_at| format!("{name} | {saved_at}"));
        for node in
            std::iter::once(entity).chain(ui_node_children.iter_descendants::<Children>(entity))
        {
            if let Ok(mut text) = ui_text_nodes.get_mut(node) {
                if text.0 != save_slot_row_label {
                    text.0.clone_from(&save_slot_row_label);
                }
            }
        }
    }
}

/// The Save dialog offers the zoo's own name to save under.
pub(super) fn offer_zoo_name_in_save_dialog(
    mut name_fields: Query<
        (
            &mut bevy::text::EditableText,
            &UiDocumentOwner,
            &openzt2_ui::authored_text_edit_bevy_adaptation::UiTextEditPolicy,
        ),
        Added<bevy::text::EditableText>,
    >,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    zoos: Query<&Name, With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>>,
) {
    let Ok(zoo) = zoos.single() else {
        return;
    };
    for (mut text, owner, policy) in &mut name_fields {
        let saves = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .is_some_and(|document| {
                document.canonical_ui_document().role == UiDocumentRole::SaveGame
            });
        if saves && policy.accepts_only_legal_filename_characters() && text.value() != zoo.as_str()
        {
            text.clear();
            text.editor_mut().set_text(zoo.as_str());
        }
    }
}

/// Choosing a save in the Save dialog puts its name in the name field, so
/// saving overwrites it and deleting names it.
pub(super) fn copy_chosen_save_name_into_save_dialog(
    rows: Query<(&ChildOf, &UiSelected, &SavedGameSource), (With<UiListRow>, Changed<UiSelected>)>,
    lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    save_slot_catalogue: Res<SaveSlotCatalogue>,
    mut name_fields: Query<(
        &mut bevy::text::EditableText,
        &UiDocumentOwner,
        &openzt2_ui::authored_text_edit_bevy_adaptation::UiTextEditPolicy,
    )>,
) {
    for (parent, selected, source) in &rows {
        let SavedGameSource::Native(slot) = *source else {
            continue;
        };
        let Some((_, document)) = lists.get(parent.parent()).ok().filter(|(policy, _)| {
            selected.0 && policy.source == UiWidgetLiveCollectionSource::SavedGameSlots
        }) else {
            continue;
        };
        let Some(record) = save_slot_catalogue
            .save_slot_records
            .iter()
            .find(|record| record.save_slot_identifier == slot)
        else {
            continue;
        };
        for (mut text, owner, policy) in &mut name_fields {
            if owner.0 == document.0
                && policy.accepts_only_legal_filename_characters()
                && text.value() != record.world_display_name.as_str()
            {
                text.clear();
                text.editor_mut().set_text(&record.world_display_name);
            }
        }
    }
}

pub(super) fn selected_save_slot_in_document(
    document_root: Entity,
    rows: &Query<(&ChildOf, &UiSelected, &SavedGameSource), With<UiListRow>>,
    lists: &Query<(&UiDocumentOwner, &UiListPolicy)>,
) -> Option<SavedGameSource> {
    rows.iter().find_map(|(parent, selected, value)| {
        (selected.0
            && lists.get(parent.parent()).is_ok_and(|(owner, policy)| {
                owner.0 == document_root
                    && policy.source == UiWidgetLiveCollectionSource::SavedGameSlots
            }))
        .then_some(*value)
    })
}

pub(super) fn load_save_slot_from_submitted_row(
    mut activations: MessageReader<UiNodeActivated>,
    rows: Query<(&ChildOf, &SavedGameSource), With<UiListRow>>,
    lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    parents: Query<&ChildOf>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut loads: MessageWriter<LoadWorldSnapshotFromSlot>,
    mut original_loads: MessageWriter<LoadOriginalSavedGame>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Submit {
            continue;
        }
        let Some(value) = std::iter::once(activation.node)
            .chain(parents.iter_ancestors::<ChildOf>(activation.node))
            .find_map(|entity| {
                let (parent, value) = rows.get(entity).ok()?;
                // Only the Load dialog loads a submitted row; the Save
                // dialog's rows press its Save button instead.
                let (policy, owner) = lists.get(parent.parent()).ok()?;
                (policy.source == UiWidgetLiveCollectionSource::SavedGameSlots
                    && presents_originals(owner.0, &roots, &documents))
                .then_some(value)
            })
        else {
            continue;
        };
        match *value {
            SavedGameSource::Native(slot) => {
                loads.write(LoadWorldSnapshotFromSlot {
                    save_slot_identifier: slot,
                });
            }
            SavedGameSource::Original(source_identifier) => {
                original_loads.write(LoadOriginalSavedGame { source_identifier });
            }
        }
    }
}

fn presents_originals(
    root: Entity,
    roots: &Query<&UiDocumentRoot>,
    documents: &Assets<UiDocumentAsset>,
) -> bool {
    roots
        .get(root)
        .ok()
        .and_then(|root| documents.get(&root.document))
        .is_some_and(|asset| asset.canonical_ui_document().role == UiDocumentRole::SavedGames)
}

pub(super) fn project_original_save_delete_control_state(
    mut commands: Commands,
    rows: Query<(&ChildOf, &UiSelected, &SavedGameSource), With<UiListRow>>,
    changed_rows: Query<
        (),
        (
            With<UiListRow>,
            Or<(Changed<UiSelected>, Changed<SavedGameSource>)>,
        ),
    >,
    mut removed_sources: RemovedComponents<SavedGameSource>,
    lists: Query<(&UiDocumentOwner, &UiListPolicy)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut document_events: MessageReader<AssetEvent<UiDocumentAsset>>,
    controls: Query<(
        Entity,
        Ref<openzt2_interface_state::ui::authored_ui_action_projection_components::UiPersistenceActions>,
        &UiDocumentOwner,
        Has<openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionRestriction>,
    )>,
) {
    use openzt2_game_data::ui_document::action::persistence::UiPersistenceAction;
    use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionRestriction;

    let selection_changed = !changed_rows.is_empty() | (removed_sources.read().count() != 0);
    let documents_changed = document_events.read().count() != 0;
    for (entity, actions, owner, restricted) in &controls {
        if !selection_changed && !documents_changed && !actions.is_changed() {
            continue;
        }
        let is_delete = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .is_some_and(|document| {
                actions.authored_action_records(document).any(|record| {
                    record.action == UiPersistenceAction::DeleteWorldSnapshotFromSelectedSlot
                })
            });
        if !is_delete {
            continue;
        }
        let restrict = matches!(
            selected_save_slot_in_document(owner.0, &rows, &lists),
            Some(SavedGameSource::Original(_))
        );
        if restrict && !restricted {
            commands.entity(entity).insert(UiInteractionRestriction);
        } else if !restrict && restricted {
            commands.entity(entity).remove::<UiInteractionRestriction>();
        }
    }
}
