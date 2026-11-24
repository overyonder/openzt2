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
    ui_list_nodes: Query<(Entity, Ref<UiListPolicy>, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut ui_list_row_count_updates: MessageWriter<SetUiListRowCount>,
) {
    let save_slot_catalogue_became_ready =
        save_slot_catalogue_ready_messages.read().next().is_some();
    for (ui_list_entity, ui_list_policy, owner) in &ui_list_nodes {
        let presents_saved_games =
            ui_list_policy.source == UiWidgetLiveCollectionSource::SavedGameSlots;
        if presents_saved_games && (save_slot_catalogue_became_ready || ui_list_policy.is_added()) {
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
    lists: Query<&UiListPolicy>,
    parents: Query<&ChildOf>,
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
                (lists.get(parent.parent()).ok()?.source
                    == UiWidgetLiveCollectionSource::SavedGameSlots)
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

#[derive(Component)]
pub(super) struct OriginalSaveDeleteControlState {
    previous_enabled: bool,
}

pub(super) fn project_original_save_delete_control_state(
    mut commands: Commands,
    rows: Query<(&ChildOf, &UiSelected, &SavedGameSource), With<UiListRow>>,
    lists: Query<(&UiDocumentOwner, &UiListPolicy)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    controls: Query<(
        Entity,
        &openzt2_interface_state::ui::authored_ui_action_projection_components::UiPersistenceActions,
        &UiDocumentOwner,
        &openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId,
        &openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled,
        Option<&OriginalSaveDeleteControlState>,
    )>,
    mut presentation: MessageWriter<
        openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction,
    >,
) {
    use openzt2_game_data::ui_document::action::{
        persistence::UiPersistenceAction, presentation::UiPresentationAction,
    };
    use openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction;
    for (entity, actions, owner, node, enabled, suppressed) in &controls {
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if !actions
            .authored_action_records(document)
            .any(|record| record.action == UiPersistenceAction::DeleteWorldSnapshotFromSelectedSlot)
        {
            continue;
        }
        let original = matches!(
            selected_save_slot_in_document(owner.0, &rows, &lists),
            Some(SavedGameSource::Original(_))
        );
        let update = match (original, suppressed) {
            (true, None) => {
                commands
                    .entity(entity)
                    .insert(OriginalSaveDeleteControlState {
                        previous_enabled: enabled.0,
                    });
                Some(false)
            }
            (false, Some(state)) => {
                commands
                    .entity(entity)
                    .remove::<OriginalSaveDeleteControlState>();
                Some(state.previous_enabled)
            }
            _ => None,
        };
        if let Some(enabled) = update {
            presentation.write(RequestUiPresentationAction {
                document_root: owner.0,
                action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                    target_node: node.id,
                    enabled,
                },
            });
        }
    }
}
