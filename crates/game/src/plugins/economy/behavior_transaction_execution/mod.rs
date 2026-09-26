//! Executes named economic actions through the existing account settlement owner.

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    behavior_transaction_state::{
        BehaviorTransactionPayment, EconomyUsage, PendingBehaviorTransaction,
    },
    facility_economy_types::FacilityPriceIndex,
    money_types::Money,
    authored_transaction_amount::{authored_currency_to_money, evaluate_authored_transaction_amount},
};
use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use crate::plugins::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::plugins::behavior_task_execution_types::advance_behavior_task_to_next_action;
use crate::plugins::behavior_task_execution_types::find_current_behavior_task_action;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure;
use crate::plugins::guests::guest_simulation_types::GuestRng;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use bevy::prelude::*;
use openzt2_game_data::{
    behavior::action_record::BehaviorAction,
    world_definitions::economy_transactions::EconomyTransactionKind,
    AssetId,
};

pub(super) fn start_authored_behavior_transactions(
    mut commands: Commands,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    tasks: Query<
        (Entity, &BehaviorTaskExecutionState),
        (
            Without<PendingBehaviorTransaction>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
) {
    for (actor, task) in &tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::Economy {
            transaction,
            cost_override,
            tracker,
            general,
        }) = assets
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, task))
        else {
            continue;
        };
        // General-manager transactions and named tracker mutations have separate
        // native recipients. Never silently apply them to the selected building.
        if *general || tracker.is_some() || task.target.is_none() {
            warn!(
                ?actor,
                ?transaction,
                "economy behavior has no supported account recipient"
            );
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        commands.entity(actor).insert(PendingBehaviorTransaction {
            execution_id: task.execution_id,
            stack_depth: task.stack.len(),
            origin: task.create_return_frame_after_action(task.action),
            facility: task.target.expect("target checked"),
            transaction: *transaction,
            cost_override: *cost_override,
            payment: None,
            next_transaction: None,
        });
    }
}

pub(super) fn execute_authored_behavior_transaction_steps(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    clock: Res<ZooClock>,
    objects: Query<(&DefinitionId, Option<&FacilityPriceIndex>)>,
    mut tasks: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &mut PendingBehaviorTransaction,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut random_streams: Query<(Option<&mut GuestRng>, Option<&mut BehaviorRandomStream>)>,
    mut payments: MessageWriter<TransactionRequest>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (actor, mut task, mut pending) in &mut tasks {
        if !pending.owns(&task) {
            commands
                .entity(actor)
                .remove::<PendingBehaviorTransaction>();
            continue;
        }
        if pending.payment.is_some() {
            continue;
        }
        let definition = objects
            .get(pending.facility)
            .ok()
            .and_then(|(id, price)| definitions.find_object(id.0).map(|object| (object, price)));
        let Some((object, price)) = definition else {
            fail_transaction(
                actor,
                pending.transaction,
                "missing facility definition",
                &mut commands,
            );
            continue;
        };
        let Some(transaction) = object
            .transactions
            .iter()
            .find(|entry| entry.name == pending.transaction)
        else {
            fail_transaction(
                actor,
                pending.transaction,
                "missing named transaction",
                &mut commands,
            );
            continue;
        };
        if transaction.target.is_some()
            || !transaction.aggregate
            || (pending.facility == actor && !transaction.track_on_parent)
        {
            fail_transaction(
                actor,
                transaction.name,
                "unmapped transaction account routing",
                &mut commands,
            );
            continue;
        }
        let price_index = (transaction.name == AssetId::from_key("buy_item"))
            .then(|| price.map(|price| usize::from(price.0)))
            .flatten();
        let mut random = random_streams.get_mut(actor).ok();
        let Some(amount) = evaluate_authored_transaction_amount(
            transaction,
            price_index,
            object.price_cents,
            pending.cost_override,
            || {
                random.as_mut().and_then(|(guest, behavior)| {
                    guest
                        .as_deref_mut()
                        .map(|random| random.0.next_u32())
                        .or_else(|| behavior.as_deref_mut().map(BehaviorRandomStream::next_u32))
                })
            },
        ) else {
            fail_transaction(
                actor,
                transaction.name,
                "unsupported or invalid transaction cost",
                &mut commands,
            );
            continue;
        };
        match transaction.kind {
            EconomyTransactionKind::Debit | EconomyTransactionKind::Credit => {
                let reversed = amount.is_sign_negative();
                let Some(amount) = authored_currency_to_money(amount.abs()) else {
                    fail_transaction(
                        actor,
                        transaction.name,
                        "transaction amount exceeds account range",
                        &mut commands,
                    );
                    continue;
                };
                if amount == Money::ZERO {
                    finish_transaction_step(
                        actor,
                        &mut task,
                        &mut pending,
                        transaction.next_transaction,
                        clock.tick,
                        &mut commands,
                    );
                    continue;
                }
                let operation = commands
                    .spawn(BehaviorTransactionPayment {
                        actor,
                        facility: pending.facility,
                        transaction: transaction.name,
                    })
                    .id();
                let owner = if transaction.track_on_parent {
                    Account::Zoo
                } else {
                    Account::External
                };
                let (debit, credit) =
                    if (transaction.kind == EconomyTransactionKind::Debit) != reversed {
                        (Account::Entity(actor), owner)
                    } else {
                        (owner, Account::Entity(actor))
                    };
                payments.write(TransactionRequest {
                    operation,
                    debit,
                    credit,
                    amount,
                    kind: TransactionKind::Service,
                    subject: Some(pending.facility),
                });
                pending.payment = Some(operation);
                pending.next_transaction = transaction.next_transaction;
            }
            EconomyTransactionKind::AddUser | EconomyTransactionKind::RemoveUser => {
                if amount < 0.0 || amount.round() >= i32::MAX as f64 {
                    fail_transaction(
                        actor,
                        transaction.name,
                        "invalid transaction user count",
                        &mut commands,
                    );
                    continue;
                }
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "finite user count checked against i32 bounds above"
                )]
                let count = amount.round() as i32;
                let adding = transaction.kind == EconomyTransactionKind::AddUser;
                queue_usage_change(actor, count, adding, &mut commands);
                if transaction.track_on_parent && pending.facility != actor {
                    queue_usage_change(pending.facility, count, adding, &mut commands);
                }
                finish_transaction_step(
                    actor,
                    &mut task,
                    &mut pending,
                    transaction.next_transaction,
                    clock.tick,
                    &mut commands,
                );
            }
            EconomyTransactionKind::SetCash => {
                fail_transaction(
                    actor,
                    transaction.name,
                    "setCash requires object current-value execution",
                    &mut commands,
                );
            }
        }
    }
}

fn queue_usage_change(entity: Entity, count: i32, adding: bool, commands: &mut Commands) {
    commands.queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
        if !entity.contains::<EconomyUsage>() {
            entity.insert(EconomyUsage::default());
        }
        if let Some(mut usage) = entity.get_mut::<EconomyUsage>() {
            usage.apply_user_change(count, adding);
        }
    });
}

fn fail_transaction(actor: Entity, transaction: AssetId, reason: &str, commands: &mut Commands) {
    warn!(
        ?actor,
        ?transaction,
        reason,
        "authored economy action failed"
    );
    mark_behavior_task_for_failure_and_stop_navigation(actor, commands);
    commands
        .entity(actor)
        .remove::<PendingBehaviorTransaction>();
}

fn finish_transaction_step(
    actor: Entity,
    task: &mut BehaviorTaskExecutionState,
    pending: &mut PendingBehaviorTransaction,
    next: Option<AssetId>,
    tick: u64,
    commands: &mut Commands,
) {
    if let Some(next) = next {
        pending.transaction = next;
        pending.payment = None;
        // Native dispatch passes the supplied override to each chained transaction.
    } else {
        advance_behavior_task_to_next_action(task, tick);
        commands
            .entity(actor)
            .remove::<PendingBehaviorTransaction>();
    }
}

pub(super) fn finish_settled_behavior_transactions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    operations: Query<&BehaviorTransactionPayment>,
    mut tasks: Query<
        (
            &mut BehaviorTaskExecutionState,
            &mut PendingBehaviorTransaction,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for payment in completed.read() {
        let Ok(owner) = operations.get(payment.operation) else {
            continue;
        };
        if let Ok((mut task, mut pending)) = tasks.get_mut(owner.actor) {
            if pending.payment == Some(payment.operation) {
                let next = pending.next_transaction;
                if pending.owns(&task) {
                    finish_transaction_step(
                        owner.actor,
                        &mut task,
                        &mut pending,
                        next,
                        clock.tick,
                        &mut commands,
                    );
                } else {
                    commands
                        .entity(owner.actor)
                        .remove::<PendingBehaviorTransaction>();
                }
            }
        }
        commands.entity(payment.operation).despawn();
    }
    for payment in rejected.read() {
        let Ok(owner) = operations.get(payment.operation) else {
            continue;
        };
        if let Ok((task, pending)) = tasks.get_mut(owner.actor) {
            if pending.payment == Some(payment.operation) {
                if pending.owns(&task) {
                    fail_transaction(
                        owner.actor,
                        pending.transaction,
                        "account settlement rejected payment",
                        &mut commands,
                    );
                } else {
                    commands
                        .entity(owner.actor)
                        .remove::<PendingBehaviorTransaction>();
                }
            }
        }
        commands.entity(payment.operation).despawn();
    }
}

pub(super) fn clear_interrupted_behavior_transaction_state(
    mut commands: Commands,
    interrupted: Query<
        Entity,
        (
            With<PendingBehaviorTransaction>,
            Or<(
                Without<BehaviorTaskExecutionState>,
                With<PendingBehaviorTaskFailure>,
            )>,
        ),
    >,
) {
    for actor in &interrupted {
        commands
            .entity(actor)
            .remove::<PendingBehaviorTransaction>();
    }
}
