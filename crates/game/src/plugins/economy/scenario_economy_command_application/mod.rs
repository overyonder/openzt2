use bevy::prelude::*;

use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    guest_admission_types::AdmissionPrice,
    money_types::Money,
    scenario_economy_command_types::{ScenarioEconomyCommand, ScenarioEconomyOperation},
};

#[derive(Component)]
pub(super) struct ScenarioEconomyCommandTransaction;

pub(super) fn apply_scenario_cash_and_admission_commands(
    mut commands: Commands,
    mut admission_price: ResMut<AdmissionPrice>,
    world_members: Query<&WorldMember>,
    mut scenario_commands: MessageReader<ScenarioEconomyCommand>,
    mut transaction_requests: MessageWriter<TransactionRequest>,
) {
    for command in scenario_commands.read() {
        if let ScenarioEconomyOperation::SetAdmission(value) = command.operation {
            admission_price.0 = value;
            continue;
        }
        let Ok(world_member) = world_members.get(command.terminal) else {
            continue;
        };

        let zoo_cash_change = match command.operation {
            ScenarioEconomyOperation::GrantCash(value) => value.0,
            ScenarioEconomyOperation::TakeCash(value) => value.0.saturating_neg(),
            ScenarioEconomyOperation::SetAdmission(_) => unreachable!(),
        };
        let (debit_account, credit_account, amount) =
            create_external_account_transaction_for_zoo_cash_change(zoo_cash_change);
        if amount == Money::ZERO {
            continue;
        }

        let transaction_operation = commands
            .spawn((*world_member, ScenarioEconomyCommandTransaction))
            .id();
        transaction_requests.write(TransactionRequest {
            operation: transaction_operation,
            debit: debit_account,
            credit: credit_account,
            amount,
            kind: TransactionKind::Reward,
            subject: None,
        });
    }
}

pub(super) fn remove_resolved_scenario_economy_command_transactions(
    mut commands: Commands,
    scenario_transactions: Query<(), With<ScenarioEconomyCommandTransaction>>,
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut rejected_transactions: MessageReader<TransactionRejected>,
) {
    completed_transactions
        .read()
        .map(|transaction| transaction.operation)
        .chain(
            rejected_transactions
                .read()
                .map(|transaction| transaction.operation),
        )
        .filter(|operation| scenario_transactions.contains(*operation))
        .for_each(|operation| commands.entity(operation).despawn());
}

fn create_external_account_transaction_for_zoo_cash_change(
    zoo_cash_change: i64,
) -> (Account, Account, Money) {
    if zoo_cash_change >= 0 {
        (Account::External, Account::Zoo, Money(zoo_cash_change))
    } else {
        (
            Account::Zoo,
            Account::External,
            Money(zoo_cash_change.saturating_abs()),
        )
    }
}
