use bevy::prelude::*;

use super::construction_edit_history_types::{ConstructionEditDirection, ConstructionEditHistory};
use super::construction_transaction_types::{EditTransaction, TransactionState};

pub(crate) fn append_applied_transaction_and_discard_redo_branch(
    commands: &mut Commands,
    construction_edit_history: &mut ConstructionEditHistory,
    applied_transaction: Entity,
) {
    for discarded_redo_transaction in construction_edit_history
        .entries
        .drain(construction_edit_history.cursor..)
    {
        if discarded_redo_transaction != applied_transaction {
            commands.entity(discarded_redo_transaction).despawn();
        }
    }

    if construction_edit_history.limit == 0 {
        commands.entity(applied_transaction).despawn();
        construction_edit_history.cursor = 0;
        return;
    }

    construction_edit_history.entries.push(applied_transaction);
    construction_edit_history.cursor = construction_edit_history.entries.len();
    if construction_edit_history.entries.len() > construction_edit_history.limit {
        let oldest_retained_transaction = construction_edit_history.entries.remove(0);
        commands.entity(oldest_retained_transaction).despawn();
        construction_edit_history.cursor -= 1;
    }
}

pub(crate) fn requested_undo_or_redo_transaction(
    construction_edit_history: &ConstructionEditHistory,
    direction: ConstructionEditDirection,
) -> Option<Entity> {
    match direction {
        ConstructionEditDirection::Undo => construction_edit_history
            .cursor
            .checked_sub(1)
            .map(|index| construction_edit_history.entries[index]),
        ConstructionEditDirection::Redo => construction_edit_history
            .entries
            .get(construction_edit_history.cursor)
            .copied(),
    }
}

pub(crate) fn accept_undo_or_redo_and_advance_history_cursor(
    construction_edit_history: &mut ConstructionEditHistory,
    transaction: Entity,
    direction: ConstructionEditDirection,
    edit_transaction: &mut EditTransaction,
) -> bool {
    match direction {
        ConstructionEditDirection::Undo
            if construction_edit_history.cursor > 0
                && construction_edit_history.entries[construction_edit_history.cursor - 1]
                    == transaction =>
        {
            construction_edit_history.cursor -= 1;
            edit_transaction.state = TransactionState::Undone;
            true
        }
        ConstructionEditDirection::Redo
            if construction_edit_history
                .entries
                .get(construction_edit_history.cursor)
                == Some(&transaction) =>
        {
            construction_edit_history.cursor += 1;
            edit_transaction.state = TransactionState::Applied;
            true
        }
        _ => false,
    }
}
