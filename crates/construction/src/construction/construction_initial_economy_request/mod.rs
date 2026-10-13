use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejection;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;

use crate::construction::construction_payment_operation_types::ConstructionPaymentPurpose;
use crate::construction::construction_payment_request_creation::create_construction_payment_request;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailure;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplicationAuthorized;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;

pub(super) fn request_initial_construction_payment_or_authorize_zero_cost_application(
    mut commands: Commands,
    mut construction_transactions: Query<
        (Entity, &WorldMember, &mut EditCommitProgress),
        Changed<EditCommitProgress>,
    >,
    mut account_transaction_requests: MessageWriter<TransactionRequest>,
    mut authorized_edit_applications: MessageWriter<EditApplicationAuthorized>,
    mut failed_construction_commits: MessageWriter<ConstructionCommitFailed>,
) {
    for (transaction_entity, world_member, mut progress) in &mut construction_transactions {
        if progress.phase != EditCommitPhase::AwaitingEconomy
            || progress.application != EditApplication::InitialCommit
        {
            continue;
        }
        if progress.cost.0 == 0 {
            progress.phase = EditCommitPhase::Applying;
            authorized_edit_applications.write(EditApplicationAuthorized {
                transaction: transaction_entity,
                application: EditApplication::InitialCommit,
            });
            continue;
        }
        let Some(account_transaction_request) = create_construction_payment_request(
            &mut commands,
            transaction_entity,
            *world_member,
            progress.cost,
            ConstructionPaymentPurpose::InitialTransfer,
            false,
        ) else {
            progress.phase = EditCommitPhase::Failed;
            failed_construction_commits.write(ConstructionCommitFailed {
                transaction: transaction_entity,
                reason: ConstructionCommitFailure::Economy(TransactionRejection::Overflow),
            });
            commands.entity(transaction_entity).despawn();
            continue;
        };
        account_transaction_requests.write(account_transaction_request);
    }
}
