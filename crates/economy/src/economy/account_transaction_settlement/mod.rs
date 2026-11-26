use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::economy::general_economy_types::{
    GeneralEconomyLedger, GeneralEconomyTransaction,
};

use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunningBehaviorEconomyTransaction, RunningBehaviorInstruction,
};

use super::{
    account_transaction_types::{
        Account, RecordedAccountMovement, TransactionCompleted, TransactionDomain,
        TransactionRejected, TransactionRejection, TransactionRequest, TransactionSettlement,
    },
    behavior_transaction_state::BehaviorTransactionPayment,
    facility_economy_types::Wallet,
    keeper_refill_settlement::{KeeperRefillPayment, KeeperRefillSettlement},
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

#[derive(Resource, Default)]
pub(super) struct AccountRequestCursor(bevy::ecs::message::MessageCursor<TransactionRequest>);

#[derive(Component)]
pub(super) struct AccountSettlementDomain<const PLAYER: bool>;

// Requests emitted between the simulation and construction boundaries are
// consumed at the latter boundary without traversing the earlier batch again.
pub(super) fn stage_requested_account_transactions<const PLAYER: bool>(
    mut commands: Commands,
    requests: Res<Messages<TransactionRequest>>,
    mut cursor: ResMut<AccountRequestCursor>,
    settlements: Query<(), With<TransactionSettlement>>,
) {
    let mut accepted = bevy::ecs::entity::EntityHashSet::default();
    for (request, message) in cursor.0.read_with_id(&requests) {
        if settlements.contains(request.operation) || !accepted.insert(request.operation) {
            continue;
        }
        let player = request.domain == TransactionDomain::Player;
        let pending = (
            AwaitingAccountSettlement,
            TransactionSettlement::Pending {
                request: *request,
                sequence: message.id,
                player_operation: player,
            },
        );
        let mut operation = commands.entity(request.operation);
        if player {
            operation.try_insert_if_new((pending, AccountSettlementDomain::<true>));
        } else {
            operation.try_insert_if_new((pending, AccountSettlementDomain::<false>));
        }
    }
}

pub(super) fn settle_requested_account_transactions<const PLAYER: bool>(
    mut commands: Commands,
    mut roots: Query<
        (
            Entity,
            &mut ZooCash,
            Option<&UnlimitedZooCash>,
            Option<&mut GeneralEconomyLedger>,
        ),
        With<WorldRoot>,
    >,
    members: Query<&WorldMember>,
    mut wallets: Query<&mut Wallet>,
    mut completed_transactions: MessageWriter<TransactionCompleted>,
    mut rejected_transactions: MessageWriter<TransactionRejected>,
    mut recorded_movements: MessageWriter<RecordedAccountMovement<PLAYER>>,
    mut transaction_operations: Query<
        (
            Entity,
            &mut TransactionSettlement,
            Option<&BehaviorTransactionPayment>,
            Option<&KeeperRefillPayment>,
            Option<&GeneralEconomyTransaction>,
        ),
        (
            With<AwaitingAccountSettlement>,
            With<AccountSettlementDomain<PLAYER>>,
        ),
    >,
    behavior_tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunningBehaviorEconomyTransaction,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    keeper_refills: KeeperRefillSettlement,
) {
    let mut pending = transaction_operations
        .iter()
        .filter_map(|(operation, state, ..)| match *state {
            TransactionSettlement::Pending { sequence, .. } => Some((sequence, operation)),
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
                let (task, instruction, pending) = behavior_tasks
                    .get(owner.actor)
                    .map_err(|_| TransactionRejection::Cancelled)?;
                if pending.payment != Some(request.operation)
                    || pending.facility != owner.facility
                    || pending.transaction != owner.transaction
                    || !pending.owns(task, instruction)
                {
                    return Err(TransactionRejection::Cancelled);
                }
                if let Some(refill) = refill {
                    keeper_refills
                        .validate(owner.actor, task, refill)
                        .ok_or(TransactionRejection::Cancelled)?;
                }
            }
            let debit_owner = match request.debit {
                Account::Entity(entity) => Some(entity),
                _ => None,
            };
            let credit_owner = match request.credit {
                Account::Entity(entity) => Some(entity),
                _ => None,
            };
            let mut owners = [Some(operation), debit_owner, credit_owner, request.subject]
                .into_iter()
                .flatten()
                .filter_map(|entity| members.get(entity).ok().map(|member| member.root));
            let root = match owners.next() {
                Some(root) if owners.all(|owner| owner == root) => root,
                Some(_) => return Err(TransactionRejection::MissingAccount),
                None => {
                    roots
                        .single()
                        .map_err(|_| TransactionRejection::MissingAccount)?
                        .0
                }
            };
            let (_, mut zoo_cash, unlimited_zoo_cash, mut general_ledger) = roots
                .get_mut(root)
                .map_err(|_| TransactionRejection::MissingAccount)?;
            let debit_account_balance = read_account_balance_for_transaction(
                request.debit,
                zoo_cash.0,
                &wallets,
                unlimited_zoo_cash.is_some_and(|policy| policy.0)
                    && matches!(request.debit, Account::Zoo),
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

            let zoo_delta = match (request.debit, request.credit) {
                (Account::Zoo, Account::Zoo) => Some(Money::ZERO),
                (Account::Zoo, _) => Money::ZERO.checked_sub(request.amount),
                (_, Account::Zoo) => Some(request.amount),
                _ => None,
            };
            let general_cash_change = if provenance.is_some()
                && general_ledger
                    .as_deref()
                    .is_some_and(|ledger| ledger.cash().is_some())
            {
                let delta = zoo_delta.unwrap_or_default();
                general_ledger
                    .as_deref()
                    .and_then(|ledger| ledger.cash_after_transaction(delta))
                    .ok_or(TransactionRejection::Overflow)?;
                Some(delta)
            } else {
                None
            };

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
            if let Some(zoo_delta) = zoo_delta {
                recorded_movements.write(RecordedAccountMovement {
                    root,
                    kind: request.kind,
                    subject: request.subject,
                    zoo_delta,
                    named_payment: behavior_payment
                        .map(|owner| (owner.facility, owner.transaction)),
                });
            }
            Ok::<Entity, TransactionRejection>(root)
        })();

        *state = TransactionSettlement::Settled {
            player_operation,
            world: settlement.as_ref().ok().copied(),
        };
        commands
            .entity(operation)
            .try_remove::<(AwaitingAccountSettlement, AccountSettlementDomain<PLAYER>)>();
        match settlement {
            Ok(_) => {
                completed_transactions.write(TransactionCompleted {
                    domain: request.domain,
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
                    domain: request.domain,
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
