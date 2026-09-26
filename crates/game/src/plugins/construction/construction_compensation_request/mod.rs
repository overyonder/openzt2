use bevy::prelude::*;

use crate::plugins::{
    economy::account_transaction_types::TransactionRequest,
    world_spawn::world_membership_types::WorldMember,
};

use super::construction_edit_history_types::{
    ConstructionEditDirection, ConstructionTransactionApplied,
};
use super::construction_payment_operation_types::ConstructionPaymentPurpose;
use super::construction_transaction_types::{
    CompensationRequested, ConstructionCommitFailed, EditApplication, EditCommitPhase,
    EditCommitProgress, EditTransaction, RollbackContext,
};
use super::{
    construction_application_failure_recovery::finish_failed_construction_application,
    construction_payment_request_creation::create_construction_payment_request,
};

pub(super) fn request_construction_undo_or_failure_compensation(
    mut commands: Commands,
    mut construction_transactions: Query<(
        Entity,
        &WorldMember,
        &EditTransaction,
        &mut EditCommitProgress,
        Option<&CompensationRequested>,
        Option<&RollbackContext>,
    )>,
    mut account_transaction_requests: MessageWriter<TransactionRequest>,
    mut applied_transactions: MessageWriter<ConstructionTransactionApplied>,
    mut failed_construction_commits: MessageWriter<ConstructionCommitFailed>,
) {
    for (
        transaction_entity,
        world_member,
        transaction,
        mut progress,
        compensation_requested,
        rollback_context,
    ) in &mut construction_transactions
    {
        if progress.phase != EditCommitPhase::AwaitingCompensation
            || compensation_requested.is_some()
        {
            continue;
        }
        let (payment_purpose, reverse_payment_direction) = match rollback_context {
            Some(_) => (ConstructionPaymentPurpose::FailureCompensation, true),
            None if progress.application == EditApplication::Undo => {
                (ConstructionPaymentPurpose::UndoCompensation, true)
            }
            None => continue,
        };
        if transaction.cost.0 == 0 {
            if let Some(rollback_context) = rollback_context {
                failed_construction_commits.write(ConstructionCommitFailed {
                    transaction: transaction_entity,
                    reason: rollback_context.failure,
                });
                finish_failed_construction_application(
                    &mut commands,
                    &mut progress,
                    transaction_entity,
                    rollback_context.failed_application,
                    &mut applied_transactions,
                );
            } else {
                progress.phase = EditCommitPhase::Complete;
                applied_transactions.write(ConstructionTransactionApplied {
                    transaction: transaction_entity,
                    direction: ConstructionEditDirection::Undo,
                    accepted: true,
                });
            }
            continue;
        }
        if let Some(account_transaction_request) = create_construction_payment_request(
            &mut commands,
            transaction_entity,
            *world_member,
            transaction.cost,
            payment_purpose,
            reverse_payment_direction,
        ) {
            commands
                .entity(transaction_entity)
                .insert(CompensationRequested);
            account_transaction_requests.write(account_transaction_request);
        }
    }
}
