use bevy::prelude::*;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::economy::general_economy_types::{
    GeneralEconomyLedger, GeneralEconomyTransaction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    complete_behavior_refill, RunningBehaviorEconomyTransaction, RunningBehaviorInstruction,
};

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRejection,
        TransactionRequest, TransactionSettlement,
    },
    behavior_transaction_state::BehaviorTransactionPayment,
    facility_economy_types::Wallet,
    keeper_refill_settlement::{KeeperRefillSettlement, StagedBehaviorRefill},
    money_types::Money,
    zoo_cash_types::{UnlimitedZooCash, ZooCash},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransactionBalanceChanges {
    debit_account_balance: Option<Money>,
    credit_account_balance: Option<Money>,
}

#[derive(Component)]
pub(super) struct AwaitingAccountSettlement;

fn calculate_transaction_balance_changes(
    amount: Money,
    accounts_are_same: bool,
    debit_account_balance: Option<Money>,
    credit_account_balance: Option<Money>,
) -> Result<TransactionBalanceChanges, TransactionRejection> {
    if !amount.is_positive() {
        return Err(TransactionRejection::NonPositive);
    }
    if accounts_are_same {
        let balance = debit_account_balance.ok_or(TransactionRejection::MissingAccount)?;
        if balance < amount {
            return Err(TransactionRejection::InsufficientFunds);
        }
        return Ok(TransactionBalanceChanges {
            debit_account_balance: Some(balance),
            credit_account_balance: Some(balance),
        });
    }

    let debit_account_balance = match debit_account_balance {
        Some(balance) if balance < amount => {
            return Err(TransactionRejection::InsufficientFunds);
        }
        Some(balance) => Some(
            balance
                .checked_sub(amount)
                .ok_or(TransactionRejection::Overflow)?,
        ),
        None => None,
    };
    let credit_account_balance = match credit_account_balance {
        Some(balance) => Some(
            balance
                .checked_add(amount)
                .ok_or(TransactionRejection::Overflow)?,
        ),
        None => None,
    };
    Ok(TransactionBalanceChanges {
        debit_account_balance,
        credit_account_balance,
    })
}

fn read_account_balance_for_transaction(
    account: Account,
    zoo_cash: Money,
    wallets: &Query<&mut Wallet>,
    allow_unlimited_zoo_debit: bool,
) -> Result<Option<Money>, TransactionRejection> {
    match account {
        Account::Zoo if allow_unlimited_zoo_debit => Ok(None),
        Account::Zoo => Ok(Some(zoo_cash)),
        Account::Entity(entity) => wallets
            .get(entity)
            .map(|wallet| Some(wallet.0))
            .map_err(|_| TransactionRejection::MissingAccount),
        Account::External => Ok(None),
    }
}

fn transaction_accounts_are_same(left_account: Account, right_account: Account) -> bool {
    match (left_account, right_account) {
        (Account::Zoo, Account::Zoo) | (Account::External, Account::External) => true,
        (Account::Entity(left_entity), Account::Entity(right_entity)) => {
            left_entity == right_entity
        }
        _ => false,
    }
}

pub(super) fn stage_requested_account_transactions<const PLAYER: bool>(
    mut commands: Commands,
    mut requests: MessageReader<TransactionRequest>,
    ui_cash_grants: Query<(), With<super::zoo_cash_ui::UiCashGrantPendingTransaction>>,
    settlements: Query<(), With<TransactionSettlement>>,
) {
    for (request, message) in requests.read_with_id() {
        let player_operation = matches!(
            request.kind,
            TransactionKind::Construction | TransactionKind::Research
        ) || ui_cash_grants.contains(request.operation);
        if player_operation == PLAYER && !settlements.contains(request.operation) {
            commands.entity(request.operation).try_insert_if_new((
                AwaitingAccountSettlement,
                TransactionSettlement::Pending {
                    request: *request,
                    sequence: message.id,
                    player_operation,
                },
            ));
        }
    }
}

pub(super) fn settle_requested_account_transactions<const PLAYER: bool>(
    mut commands: Commands,
    mut zoo_cash: ResMut<ZooCash>,
    unlimited_zoo_cash: Option<Res<UnlimitedZooCash>>,
    mut general_ledger: Option<ResMut<GeneralEconomyLedger>>,
    calendar: Option<Res<ZooCalendar>>,
    mut wallets: Query<&mut Wallet>,
    mut completed_transactions: MessageWriter<TransactionCompleted>,
    mut rejected_transactions: MessageWriter<TransactionRejected>,
    mut transaction_operations: Query<
        (
            Entity,
            &mut TransactionSettlement,
            Option<&BehaviorTransactionPayment>,
            Option<&StagedBehaviorRefill>,
            Option<&GeneralEconomyTransaction>,
        ),
        With<AwaitingAccountSettlement>,
    >,
    behavior_tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunningBehaviorEconomyTransaction,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut keeper_refills: KeeperRefillSettlement,
    clock: Option<Res<ZooClock>>,
) {
    if let (Some(ledger), Some(calendar)) = (general_ledger.as_deref_mut(), calendar.as_deref()) {
        ledger.enter_calendar_month(
            u32::from(calendar.year) * 12 + u32::from(calendar.month.saturating_sub(1)),
        );
    }
    let mut pending = transaction_operations
        .iter()
        .filter_map(|(operation, state, ..)| match *state {
            TransactionSettlement::Pending {
                sequence,
                player_operation,
                ..
            } if player_operation == PLAYER => Some((sequence, operation)),
            _ => None,
        })
        .collect::<Vec<_>>();
    pending.sort_unstable();
    for (_, operation) in pending {
        let Ok((_, mut state, behavior_payment, refill, provenance)) =
            transaction_operations.get_mut(operation)
        else {
            continue;
        };
        let TransactionSettlement::Pending {
            request,
            player_operation,
            ..
        } = *state
        else {
            continue;
        };

        let settlement = (|| {
            // Authored payments are valid only while the originating instruction
            // still owns the request. Act -> Economy may cross a failure/restart.
            // Other producers retain their existing settlement contracts.
            if let Some(owner) = behavior_payment {
                let valid =
                    behavior_tasks
                        .get(owner.actor)
                        .is_ok_and(|(task, instruction, pending)| {
                            pending.payment == Some(request.operation)
                                && pending.facility == owner.facility
                                && pending.transaction == owner.transaction
                                && pending.owns(task, instruction)
                        });
                if !valid {
                    return Err(TransactionRejection::Cancelled);
                }
            }
            let debit_account_balance = read_account_balance_for_transaction(
                request.debit,
                zoo_cash.0,
                &wallets,
                unlimited_zoo_cash.is_some() && matches!(request.debit, Account::Zoo),
            )?;
            let credit_account_balance =
                read_account_balance_for_transaction(request.credit, zoo_cash.0, &wallets, false)?;
            let accounts_are_same = transaction_accounts_are_same(request.debit, request.credit);
            let balance_changes = calculate_transaction_balance_changes(
                request.amount,
                accounts_are_same,
                debit_account_balance,
                credit_account_balance,
            )?;

            let general_cash_change = if provenance.is_some()
                && general_ledger
                    .as_deref()
                    .is_some_and(|ledger| ledger.cash().is_some())
            {
                let delta = match (request.debit, request.credit) {
                    (Account::Zoo, Account::Zoo) => 0,
                    (Account::Zoo, _) => -request.amount.0,
                    (_, Account::Zoo) => request.amount.0,
                    _ => 0,
                };
                general_ledger
                    .as_deref()
                    .and_then(|ledger| ledger.cash_after_transaction(delta))
                    .ok_or(TransactionRejection::Overflow)?;
                Some(delta)
            } else {
                None
            };

            if let (Some(owner), Some(refill)) = (behavior_payment, refill) {
                let (task, instruction, _) = behavior_tasks
                    .get(owner.actor)
                    .map_err(|_| TransactionRejection::Cancelled)?;
                let tick = clock.as_ref().ok_or(TransactionRejection::Cancelled)?.tick;
                keeper_refills
                    .commit(owner.actor, task, refill)
                    .ok_or(TransactionRejection::Cancelled)?;
                complete_behavior_refill(
                    owner.actor,
                    instruction.id(),
                    tick,
                    refill.continuation,
                    &mut commands,
                );
            }

            if let Some(delta) = general_cash_change {
                general_ledger
                    .as_deref_mut()
                    .expect("general balance was checked")
                    .record_completed_transaction(delta)
                    .expect("general movement was checked");
            }

            if !accounts_are_same {
                if let Some(balance) = balance_changes.debit_account_balance {
                    match request.debit {
                        Account::Zoo => zoo_cash.0 = balance,
                        Account::Entity(entity) => {
                            wallets
                                .get_mut(entity)
                                .expect("account existence was checked")
                                .0 = balance;
                        }
                        Account::External => {}
                    }
                }
                if let Some(balance) = balance_changes.credit_account_balance {
                    match request.credit {
                        Account::Zoo => zoo_cash.0 = balance,
                        Account::Entity(entity) => {
                            wallets
                                .get_mut(entity)
                                .expect("account existence was checked")
                                .0 = balance;
                        }
                        Account::External => {}
                    }
                }
            }
            Ok::<(), TransactionRejection>(())
        })();

        *state = TransactionSettlement::Settled { player_operation };
        commands
            .entity(operation)
            .try_remove::<AwaitingAccountSettlement>();
        match settlement {
            Ok(()) => {
                completed_transactions.write(TransactionCompleted {
                    operation: request.operation,
                    debit: request.debit,
                    credit: request.credit,
                    amount: request.amount,
                    kind: request.kind,
                    subject: request.subject,
                });
            }
            Err(reason) => {
                rejected_transactions.write(TransactionRejected {
                    operation: request.operation,
                    debit: request.debit,
                    credit: request.credit,
                    amount: request.amount,
                    kind: request.kind,
                    subject: request.subject,
                    reason,
                });
            }
        }
    }
}
