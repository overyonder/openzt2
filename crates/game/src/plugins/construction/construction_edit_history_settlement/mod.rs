use bevy::prelude::*;

use super::construction_edit_history_operations;
use super::construction_edit_history_types::{
    ConstructionEditHistory, ConstructionTransactionApplied, RecordAppliedConstructionTransaction,
};
use super::construction_transaction_types::{EditTransaction, TransactionState};

pub(super) fn record_applied_construction_transactions_in_history(
    mut commands: Commands,
    mut transaction_record_requests: MessageReader<RecordAppliedConstructionTransaction>,
    mut construction_edit_history: ResMut<ConstructionEditHistory>,
    transactions: Query<&EditTransaction>,
) {
    for transaction_record_request in transaction_record_requests.read() {
        let Ok(transaction) = transactions.get(transaction_record_request.0) else {
            continue;
        };
        if transaction.state != TransactionState::Applied {
            continue;
        }
        construction_edit_history_operations::append_applied_transaction_and_discard_redo_branch(
            &mut commands,
            &mut construction_edit_history,
            transaction_record_request.0,
        );
    }
}

pub(super) fn advance_construction_edit_history_after_accepted_application(
    mut transaction_application_results: MessageReader<ConstructionTransactionApplied>,
    mut construction_edit_history: ResMut<ConstructionEditHistory>,
    mut transactions: Query<&mut EditTransaction>,
) {
    for transaction_application_result in transaction_application_results.read() {
        if !transaction_application_result.accepted {
            continue;
        }
        let Ok(mut transaction) = transactions.get_mut(transaction_application_result.transaction)
        else {
            continue;
        };
        construction_edit_history_operations::accept_undo_or_redo_and_advance_history_cursor(
            &mut construction_edit_history,
            transaction_application_result.transaction,
            transaction_application_result.direction,
            &mut transaction,
        );
    }
}
