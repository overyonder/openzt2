use bevy::prelude::*;

use crate::plugins::economy::account_transaction_types::{
    TransactionCompleted, TransactionRejected,
};

use super::construction_application_failure_recovery::{
    begin_construction_application_rollback,
    construction_domain_status_after_rejected_redo_payment, finish_failed_construction_application,
};
use super::construction_edit_history_types::{
    ConstructionEditDirection, ConstructionTransactionApplied,
};
use super::construction_payment_operation_types::{
    ConstructionPaymentOperation, ConstructionPaymentPurpose,
};
use super::construction_transaction_types::{
    CompensationRequested, ConstructionCommitFailed, ConstructionCommitFailure, EditApplication,
    EditApplicationAuthorized, EditCommitPhase, EditCommitProgress, RollbackContext,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn complete_construction_payment_results(
    mut commands: Commands,
    mut completed_account_transactions: MessageReader<TransactionCompleted>,
    mut rejected_account_transactions: MessageReader<TransactionRejected>,
    construction_payments: Query<&ConstructionPaymentOperation>,
    rollback_contexts: Query<&RollbackContext>,
    mut construction_transactions: Query<&mut EditCommitProgress>,
    mut authorized_edit_applications: MessageWriter<EditApplicationAuthorized>,
    mut applied_transactions: MessageWriter<ConstructionTransactionApplied>,
    mut failed_construction_commits: MessageWriter<ConstructionCommitFailed>,
) {
    for completed_account_transaction in completed_account_transactions.read() {
        let Ok(construction_payment) =
            construction_payments.get(completed_account_transaction.operation)
        else {
            continue;
        };
        let Ok(mut progress) =
            construction_transactions.get_mut(construction_payment.construction_transaction)
        else {
            commands
                .entity(completed_account_transaction.operation)
                .despawn();
            continue;
        };
        match construction_payment.purpose {
            ConstructionPaymentPurpose::InitialTransfer
            | ConstructionPaymentPurpose::RedoTransfer => {
                let application = if construction_payment.purpose
                    == ConstructionPaymentPurpose::InitialTransfer
                {
                    EditApplication::InitialCommit
                } else {
                    EditApplication::Redo
                };
                if progress.phase == EditCommitPhase::AwaitingEconomy
                    && progress.application == application
                {
                    progress.phase = EditCommitPhase::Applying;
                    authorized_edit_applications.write(EditApplicationAuthorized {
                        transaction: construction_payment.construction_transaction,
                        application,
                    });
                }
            }
            ConstructionPaymentPurpose::UndoCompensation => {
                if progress.phase == EditCommitPhase::AwaitingCompensation {
                    progress.phase = EditCommitPhase::Complete;
                    applied_transactions.write(ConstructionTransactionApplied {
                        transaction: construction_payment.construction_transaction,
                        direction: ConstructionEditDirection::Undo,
                        accepted: true,
                    });
                    commands
                        .entity(construction_payment.construction_transaction)
                        .remove::<CompensationRequested>();
                }
            }
            ConstructionPaymentPurpose::FailureCompensation => {
                if progress.phase == EditCommitPhase::AwaitingCompensation {
                    let Ok(rollback_context) =
                        rollback_contexts.get(construction_payment.construction_transaction)
                    else {
                        continue;
                    };
                    failed_construction_commits.write(ConstructionCommitFailed {
                        transaction: construction_payment.construction_transaction,
                        reason: rollback_context.failure,
                    });
                    finish_failed_construction_application(
                        &mut commands,
                        &mut progress,
                        construction_payment.construction_transaction,
                        rollback_context.failed_application,
                        &mut applied_transactions,
                    );
                }
            }
        }
        commands
            .entity(completed_account_transaction.operation)
            .despawn();
    }

    for rejected_account_transaction in rejected_account_transactions.read() {
        let Ok(construction_payment) =
            construction_payments.get(rejected_account_transaction.operation)
        else {
            continue;
        };
        let Ok(mut progress) =
            construction_transactions.get_mut(construction_payment.construction_transaction)
        else {
            commands
                .entity(rejected_account_transaction.operation)
                .despawn();
            continue;
        };
        match construction_payment.purpose {
            ConstructionPaymentPurpose::InitialTransfer => {
                progress.phase = EditCommitPhase::Failed;
                failed_construction_commits.write(ConstructionCommitFailed {
                    transaction: construction_payment.construction_transaction,
                    reason: ConstructionCommitFailure::Economy(rejected_account_transaction.reason),
                });
                commands
                    .entity(construction_payment.construction_transaction)
                    .despawn();
            }
            ConstructionPaymentPurpose::RedoTransfer => {
                progress.terrain =
                    construction_domain_status_after_rejected_redo_payment(progress.terrain);
                progress.topology =
                    construction_domain_status_after_rejected_redo_payment(progress.topology);
                progress.placement =
                    construction_domain_status_after_rejected_redo_payment(progress.placement);
                progress.shows =
                    construction_domain_status_after_rejected_redo_payment(progress.shows);
                progress.phase = EditCommitPhase::Complete;
                applied_transactions.write(ConstructionTransactionApplied {
                    transaction: construction_payment.construction_transaction,
                    direction: ConstructionEditDirection::Redo,
                    accepted: false,
                });
                failed_construction_commits.write(ConstructionCommitFailed {
                    transaction: construction_payment.construction_transaction,
                    reason: ConstructionCommitFailure::Economy(rejected_account_transaction.reason),
                });
            }
            ConstructionPaymentPurpose::UndoCompensation => {
                begin_construction_application_rollback(
                    &mut commands,
                    construction_payment.construction_transaction,
                    &mut progress,
                    EditApplication::Undo,
                    ConstructionCommitFailure::Compensation(rejected_account_transaction.reason),
                );
            }
            ConstructionPaymentPurpose::FailureCompensation => {
                progress.phase = EditCommitPhase::Failed;
                failed_construction_commits.write(ConstructionCommitFailed {
                    transaction: construction_payment.construction_transaction,
                    reason: ConstructionCommitFailure::Compensation(
                        rejected_account_transaction.reason,
                    ),
                });
            }
        }
        commands
            .entity(rejected_account_transaction.operation)
            .despawn();
    }
}
