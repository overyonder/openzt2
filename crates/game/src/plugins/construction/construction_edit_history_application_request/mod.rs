use bevy::prelude::*;

use crate::plugins::{
    economy::account_transaction_types::{TransactionRejection, TransactionRequest},
    world_spawn::world_membership_types::WorldMember,
};

use super::construction_edit_history_types::{
    ApplyConstructionTransaction, ConstructionEditDirection, ConstructionEditHistory,
};
use super::construction_payment_operation_types::ConstructionPaymentPurpose;
use super::construction_transaction_types::{
    ConstructionCommitFailed, ConstructionCommitFailure, EditApplication,
    EditApplicationAuthorized, EditCommitPhase, EditCommitProgress, EditTransaction,
    TransactionState,
};
use super::{
    construction_application_failure_recovery::construction_domain_status_after_rejected_redo_payment,
    construction_edit_history_operations,
    construction_payment_request_creation::create_construction_payment_request,
};

pub(super) fn begin_requested_construction_edit_history_application(
    mut commands: Commands,
    mut requested_history_applications: MessageReader<ApplyConstructionTransaction>,
    construction_edit_history: Res<ConstructionEditHistory>,
    mut construction_transactions: Query<(&EditTransaction, &WorldMember, &mut EditCommitProgress)>,
    mut account_transaction_requests: MessageWriter<TransactionRequest>,
    mut authorized_edit_applications: MessageWriter<EditApplicationAuthorized>,
    mut failed_construction_commits: MessageWriter<ConstructionCommitFailed>,
) {
    if construction_transactions
        .iter_mut()
        .any(|(_, _, progress)| progress.phase != EditCommitPhase::Complete)
    {
        return;
    }
    for requested_application in requested_history_applications.read() {
        if construction_edit_history_operations::requested_undo_or_redo_transaction(
            &construction_edit_history,
            requested_application.direction,
        ) != Some(requested_application.transaction)
        {
            continue;
        }
        let Ok((transaction, world_member, mut progress)) =
            construction_transactions.get_mut(requested_application.transaction)
        else {
            continue;
        };
        let application = match (requested_application.direction, transaction.state) {
            (ConstructionEditDirection::Undo, TransactionState::Applied) => EditApplication::Undo,
            (ConstructionEditDirection::Redo, TransactionState::Undone) => EditApplication::Redo,
            _ => continue,
        };
        progress.application = application;
        progress.reset_expected_to_pending();
        if application == EditApplication::Undo || progress.cost.0 == 0 {
            progress.phase = EditCommitPhase::Applying;
            authorized_edit_applications.write(EditApplicationAuthorized {
                transaction: requested_application.transaction,
                application,
            });
        } else {
            match create_construction_payment_request(
                &mut commands,
                requested_application.transaction,
                *world_member,
                progress.cost,
                ConstructionPaymentPurpose::RedoTransfer,
                false,
            ) {
                Some(account_transaction_request) => {
                    progress.phase = EditCommitPhase::AwaitingEconomy;
                    account_transaction_requests.write(account_transaction_request);
                }
                None => {
                    progress.terrain =
                        construction_domain_status_after_rejected_redo_payment(progress.terrain);
                    progress.topology =
                        construction_domain_status_after_rejected_redo_payment(progress.topology);
                    progress.placement =
                        construction_domain_status_after_rejected_redo_payment(progress.placement);
                    progress.shows =
                        construction_domain_status_after_rejected_redo_payment(progress.shows);
                    progress.phase = EditCommitPhase::Complete;
                    failed_construction_commits.write(ConstructionCommitFailed {
                        transaction: requested_application.transaction,
                        reason: ConstructionCommitFailure::Economy(TransactionRejection::Overflow),
                    });
                }
            }
        }
        break;
    }
}
