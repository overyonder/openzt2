use bevy::prelude::*;

use crate::plugins::{
    economy::money_types::Money, world_spawn::world_membership_types::WorldMember,
};

use super::construction_edit_history_types::ConstructionEditHistory;
use super::construction_interaction_types::DeleteEntity;
use super::construction_transaction_types::{
    EditCommitPhase, EditCommitProgress, EditTransaction, PrepareDeletion, TransactionState,
};

pub(crate) fn begin_deletion_transaction_preparation(
    mut commands: Commands,
    mut deletion_requests: MessageReader<DeleteEntity>,
    pending_transactions: Query<&EditCommitProgress>,
    world_members: Query<&WorldMember>,
    mut construction_edit_history: ResMut<ConstructionEditHistory>,
    mut deletion_preparation_requests: MessageWriter<PrepareDeletion>,
) {
    if pending_transactions
        .iter()
        .any(|progress| progress.phase != EditCommitPhase::Complete)
    {
        return;
    }
    for deletion_request in deletion_requests.read() {
        let Ok(world_member) = world_members.get(deletion_request.0) else {
            continue;
        };
        let Some(transaction_sequence) = construction_edit_history.allocate_sequence() else {
            continue;
        };
        let transaction = commands
            .spawn((
                EditTransaction {
                    sequence: transaction_sequence,
                    state: TransactionState::Applied,
                    cost: Money(0),
                },
                EditCommitProgress::preparing(),
                *world_member,
            ))
            .id();
        deletion_preparation_requests.write(PrepareDeletion {
            transaction,
            target: deletion_request.0,
        });
        break;
    }
}
