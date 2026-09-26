use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};
use bevy::{platform::collections::HashSet, prelude::*};

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionRejected, TransactionRejection,
        TransactionRequest, TransactionSettled,
    },
    behavior_transaction_state::{BehaviorTransactionPayment, PendingBehaviorTransaction},
    facility_economy_types::Wallet,
    money_types::Money,
    zoo_cash_types::{UnlimitedZooCash, ZooCash},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransactionBalanceChanges {
    debit_account_balance: Option<Money>,
    credit_account_balance: Option<Money>,
}

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

pub(super) fn settle_requested_account_transactions(
    mut commands: Commands,
    mut transaction_requests: MessageReader<TransactionRequest>,
    mut zoo_cash: ResMut<ZooCash>,
    unlimited_zoo_cash: Option<Res<UnlimitedZooCash>>,
    mut wallets: Query<&mut Wallet>,
    mut completed_transactions: MessageWriter<TransactionCompleted>,
    mut rejected_transactions: MessageWriter<TransactionRejected>,
    transaction_operations: Query<(
        Option<&TransactionSettled>,
        Option<&BehaviorTransactionPayment>,
    )>,
    behavior_tasks: Query<
        (&BehaviorTaskExecutionState, &PendingBehaviorTransaction),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut operations_settled_this_tick: Local<HashSet<Entity>>,
) {
    operations_settled_this_tick.clear();
    for request in transaction_requests.read() {
        if transaction_operations
            .get(request.operation)
            .is_ok_and(|(settled, _)| settled.is_some())
            || !operations_settled_this_tick.insert(request.operation)
        {
            continue;
        }

        let settlement = (|| {
            // Authored payments are valid only while the originating instruction
            // still owns the request. Act -> Economy may cross a failure/restart.
            // Other producers retain their existing settlement contracts.
            if let Ok((_, Some(owner))) = transaction_operations.get(request.operation) {
                let valid = behavior_tasks
                    .get(owner.actor)
                    .is_ok_and(|(task, pending)| {
                        pending.payment == Some(request.operation)
                            && pending.facility == owner.facility
                            && pending.transaction == owner.transaction
                            && pending.owns(task)
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

        // A producer may reserve and spawn the operation through deferred
        // Commands in the same schedule. The per-tick set is authoritative in
        // that case; established operations also retain a replay marker.
        if transaction_operations.get(request.operation).is_ok() {
            commands
                .entity(request.operation)
                .insert(TransactionSettled);
        }
    }
}
