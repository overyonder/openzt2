use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::construction::UiConstructionAction;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiConstructionActions;
use openzt2_interface_state::ui::authored_ui_focus_state::UiFocusable;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;

use crate::construction::construction_edit_history_operations;

pub(super) fn project_construction_undo_availability(
    history: Res<ConstructionEditHistory>,
    pending_transactions: Query<&EditCommitProgress>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut controls: Query<(
        &UiConstructionActions,
        &UiDocumentOwner,
        &mut UiInteractionEnabled,
        Option<&mut UiFocusable>,
        &mut Interaction,
    )>,
) {
    let undo_available = construction_edit_history_operations::requested_undo_or_redo_transaction(
        &history,
        ConstructionEditDirection::Undo,
    )
    .is_some()
        && pending_transactions
            .iter()
            .all(|progress| progress.phase == EditCommitPhase::Complete);

    for (actions, owner, mut enabled, focusable, mut interaction) in &mut controls {
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if !actions
            .authored_action_records(document)
            .any(|record| matches!(&record.action, UiConstructionAction::Undo))
        {
            continue;
        }
        if enabled.0 != undo_available {
            enabled.0 = undo_available;
        }
        if let Some(mut focusable) = focusable {
            if focusable.enabled != undo_available {
                focusable.enabled = undo_available;
            }
        }
        if !undo_available && *interaction != Interaction::None {
            *interaction = Interaction::None;
        }
    }
}
