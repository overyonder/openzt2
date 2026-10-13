use bevy::prelude::*;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;

use crate::construction::construction_edit_history_operations;
use openzt2_zoo_state::construction::construction_edit_history_types::ApplyConstructionTransaction;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;

pub(super) fn no_pending_edits(pending: Query<&EditCommitProgress>) -> bool {
    pending
        .iter()
        .all(|progress| progress.phase == EditCommitPhase::Complete)
}

pub(super) fn request_undo_redo(
    mut actions: MessageReader<ActionRequest>,
    history: Res<ConstructionEditHistory>,
    mut apply: MessageWriter<ApplyConstructionTransaction>,
) {
    for request in actions.read() {
        let direction = match request.action {
            GameAction::Undo => ConstructionEditDirection::Undo,
            GameAction::Redo => ConstructionEditDirection::Redo,
            _ => continue,
        };
        if let Some(transaction) =
            construction_edit_history_operations::requested_undo_or_redo_transaction(
                &history, direction,
            )
        {
            apply.write(ApplyConstructionTransaction {
                transaction,
                direction,
            });
            break;
        }
    }
}
