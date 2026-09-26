use bevy::prelude::*;

use crate::plugins::input::input_types::{ActionRequest, GameAction};

use super::{
    construction_edit_history_operations,
    construction_edit_history_types::{
        ApplyConstructionTransaction, ConstructionEditDirection, ConstructionEditHistory,
    },
    construction_transaction_types::{EditCommitPhase, EditCommitProgress},
};

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
