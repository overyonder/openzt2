use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejection;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;

use crate::construction::construction_application_failure_recovery::construction_domain_status_after_rejected_redo_payment;
use crate::construction::construction_edit_history_operations;
use crate::construction::construction_payment_operation_types::ConstructionPaymentPurpose;
use crate::construction::construction_payment_request_creation::create_construction_payment_request;
use openzt2_zoo_state::construction::construction_edit_history_types::ApplyConstructionTransaction;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailure;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplicationAuthorized;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

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
                    progress.aquatic =
                        construction_domain_status_after_rejected_redo_payment(progress.aquatic);
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
