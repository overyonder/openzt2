use super::{
    construction_history::WorldConstructionHistory,
    construction_lifecycle::{ConstructionActive, Preparing},
};
use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::economy::money_types::Money;

use openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

pub fn begin_deletion_transaction_preparation(
    mut commands: Commands,
    mut deletion_requests: MessageReader<DeleteEntity>,
    pending_transactions: Query<(), With<ConstructionActive>>,
    world_members: Query<&WorldMember>,
    mut world_histories: Query<&mut WorldConstructionHistory, With<WorldRoot>>,
    mut deletion_preparation_requests: MessageWriter<PrepareDeletion>,
) {
    if !pending_transactions.is_empty() {
        return;
    }
    for deletion_request in deletion_requests.read() {
        let Ok(world_member) = world_members.get(deletion_request.0) else {
            continue;
        };
        let Ok(mut history) = world_histories.get_mut(world_member.root) else {
            continue;
        };
        let Some(transaction_sequence) = history.allocate_sequence() else {
            continue;
        };
        let transaction = commands
            .spawn((
                EditTransaction {
                    sequence: transaction_sequence,
                    state: TransactionState::Applied,
                    cost: Money(0),
                },
                Preparing,
                *world_member,
                openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction::Destroy,
            ))
            .id();
        deletion_preparation_requests.write(PrepareDeletion {
            transaction,
            target: deletion_request.0,
        });
        break;
    }
}
