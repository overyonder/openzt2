use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::money_types::Money;

use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

pub fn begin_deletion_transaction_preparation(
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
