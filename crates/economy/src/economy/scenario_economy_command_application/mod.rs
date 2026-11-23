use bevy::prelude::*;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction;

use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

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
    mut admission: Query<&mut AdmissionPrice, With<WorldRoot>>,
    world_members: Query<&WorldMember>,
    mut scenario_commands: MessageReader<ScenarioEconomyCommand>,
    mut transaction_requests: MessageWriter<TransactionRequest>,
) {
    for command in scenario_commands.read() {
        let root = world_members
            .get(command.terminal)
            .map_or(command.terminal, |member| member.root);
        if let ScenarioEconomyOperation::SetAdmission(value) = command.operation {
            if let Ok(mut admission_price) = admission.get_mut(root) {
                admission_price.set_if_neq(AdmissionPrice::from_adult(value));
            }
            continue;
        }

        if !admission.contains(root) {
            continue;
        }
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
            .spawn((
                WorldMember { root },
                ScenarioEconomyCommandTransaction,
                GeneralEconomyTransaction::CashGrant,
            ))
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
