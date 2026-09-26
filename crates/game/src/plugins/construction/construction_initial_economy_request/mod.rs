use bevy::prelude::*;

use crate::plugins::{
    economy::account_transaction_types::{TransactionRejection, TransactionRequest},
    world_spawn::world_membership_types::WorldMember,
};

use super::construction_payment_operation_types::ConstructionPaymentPurpose;
use super::construction_payment_request_creation::create_construction_payment_request;
use super::construction_transaction_types::{
    ConstructionCommitFailed, ConstructionCommitFailure, EditApplication,
    EditApplicationAuthorized, EditCommitPhase, EditCommitProgress,
};

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
