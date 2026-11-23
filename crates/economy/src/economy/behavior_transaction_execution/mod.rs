//! Executes named economic actions through the existing account settlement owner.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::{RunBehaviorEconomy, RunningBehaviorInstruction, RunningBehaviorEconomyTransaction, WhenBehaviorAction, behavior_action_actors};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorEconomyTracker;
use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    behavior_transaction_state::{
        BehaviorTransactionPayment, EconomyUsage,
    },
    facility_economy_types::{FacilityPriceIndex, Wallet},
    money_types::Money,
    authored_transaction_amount::{authored_currency_to_money, evaluate_authored_transaction_amount},
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::{
    world_definitions::economy_transactions::EconomyTransactionKind,
    AssetId,
};

pub(super) fn start_authored_behavior_transactions(
    activation: On<WhenBehaviorAction<RunBehaviorEconomy>>,
    mut commands: Commands,
    show_donations: Query<
        &openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken,
        With<openzt2_zoo_state::economy::experience_donation::ExperienceDonation>,
    >,
    tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorEconomy,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let actor = activation.entity;
    let Ok((task, instruction, running_action)) = tasks.get(actor) else {
        return;
    };
    if instruction.id() != activation.instruction || !instruction.is_current(task) {
        return;
    }
    if running_action.transaction == AssetId::from_key("donate")
        && task
            .object
            .and_then(|object| show_donations.get(object).ok())
            .is_some_and(|token| token.name == AssetId::from_key("t_donationshow"))
    {
        return;
    }
    commands
        .entity(actor)
        .insert(RunningBehaviorEconomyTransaction {
            instruction: instruction.id(),
            facility: task.target.unwrap_or(actor),
            target: task.target,
            transaction: running_action.transaction,
            cost_override: running_action.cost_override,
            payment: None,
            next_transaction: None,
        });
}

pub(super) fn added_behavior_transaction(
    added: On<Add<RunningBehaviorEconomyTransaction>>,
) -> Option<Entity> {
    Some(added.entity)
}

pub(super) fn execute_authored_behavior_transaction_steps(
    In(activated_actor): In<Option<Entity>>,
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    clocks: Query<&ZooClock>,
    objects: Query<(&DefinitionId, Option<&FacilityPriceIndex>)>,
    worlds: Query<&WorldMember>,
    tracker_indexes: Query<(
        &openzt2_zoo_state::economy::world_economy_configuration::WorldEconomyConfiguration,
        Option<&super::saved_world_economy_trackers::WorldEconomyTrackerIndex>,
    )>,
    asset_server: Res<AssetServer>,
    trackers: Query<&BehaviorEconomyTracker>,
    mut wallets: Query<&mut Wallet>,
    mut tasks: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorEconomy,
            &mut RunningBehaviorEconomyTransaction,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut random_streams: Query<(Option<&mut GuestRng>, Option<&mut BehaviorRandomStream>)>,
    mut payments: MessageWriter<TransactionRequest>,
    staff_jobs: Query<&CurrentJob, With<Staff>>,
    mut usage_counters: Query<&mut EconomyUsage>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let mut new_usage = HashMap::<Entity, EconomyUsage>::new();
    for (actor, task, instruction, running, mut pending) in
        behavior_action_actors(&mut tasks, activated_actor)
    {
        let Ok(member) = worlds.get(actor) else {
            continue;
        };
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        if !pending.owns(task, instruction) {
            commands
                .entity(actor)
                .remove::<RunningBehaviorEconomyTransaction>();
            continue;
        }
        if pending.payment.is_some()
            || task.next_action_tick > clock.tick
            || (activated_actor.is_none() && !instruction.can_update_after_activation(clock.tick))
        {
            continue;
        }
        let tracker = if running.general || running.tracker.is_some() {
            let key = running
                .tracker
                .unwrap_or_else(|| AssetId::from_key("general"));
            let configuration = tracker_indexes.get(member.root).ok();
            let entity = configuration.and_then(|(_, index)| index?.0.get(&key).copied());
            let Some((entity, tracker)) = entity
                .and_then(|entity| trackers.get(entity).ok().map(|tracker| (entity, tracker)))
            else {
                if configuration.is_some_and(|(configuration, index)| {
                    index.is_none()
                        && !matches!(
                            asset_server.load_state(configuration.0.id()),
                            bevy::asset::LoadState::Failed(_)
                        )
                }) {
                    continue;
                }
                fail_transaction(
                    actor,
                    pending.transaction,
                    "missing authored transaction tracker",
                    &mut commands,
                );
                continue;
            };
            Some((entity, tracker))
        } else {
            None
        };
        let object = objects
            .get(pending.facility)
            .ok()
            .and_then(|(id, price)| definitions.find_object(id.0).map(|object| (object, price)));
        let records = tracker
            .map(|(_, tracker)| tracker.transactions.as_slice())
            .or_else(|| {
                object
                    .as_ref()
                    .map(|(object, _)| object.transactions.as_slice())
            });
        let Some(transaction) = records.and_then(|records| {
            records
                .iter()
                .find(|entry| entry.name == pending.transaction)
        }) else {
            fail_transaction(
                actor,
                pending.transaction,
                "missing named transaction",
                &mut commands,
            );
            continue;
        };
        let base_price = object.as_ref().map_or(0, |(object, _)| object.price_cents);
        let price = object.and_then(|(_, price)| price);
        let account_recipient = if let Some(key) = transaction.target {
            if key == AssetId::from_key("subject") {
                Some(actor)
            } else if key == AssetId::from_key("target") {
                task.target
            } else if key == AssetId::from_key("object") {
                task.object
            } else {
                [Some(actor), task.target, task.object]
                    .into_iter()
                    .flatten()
                    .find(|entity| {
                        objects
                            .get(*entity)
                            .is_ok_and(|(definition, _)| definition.0 == key)
                    })
            }
        } else {
            Some(actor)
        };
        let Some(account_recipient) = account_recipient else {
            fail_transaction(
                actor,
                pending.transaction,
                "missing explicit transaction account",
                &mut commands,
            );
            continue;
        };
        let price_index = (transaction.name == AssetId::from_key("buy_item"))
            .then(|| price.map(|price| usize::from(price.0)))
            .flatten();
        let mut random = random_streams.get_mut(actor).ok();
        let Some(amount) = evaluate_authored_transaction_amount(
            transaction,
            price_index,
            base_price,
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
        let keeper_refill = (pending.transaction == AssetId::from_key("refill"))
            .then(|| staff_jobs.get(actor).ok())
            .flatten();
        if keeper_refill.is_some()
            && (transaction.kind != EconomyTransactionKind::Credit
                || !transaction.track_on_parent
                || transaction.next_transaction.is_some()
                || object.is_none()
                || running.keeper_refill.is_none()
                || amount <= 0.0)
        {
            fail_transaction(
                actor,
                transaction.name,
                "unsupported keeper refill charge",
                &mut commands,
            );
            continue;
        }
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
                    if keeper_refill.is_some() {
                        fail_transaction(
                            actor,
                            transaction.name,
                            "keeper refill charge rounds to zero",
                            &mut commands,
                        );
                        continue;
                    }
                    finish_transaction_step(
                        actor,
                        instruction,
                        &mut pending,
                        transaction.next_transaction,
                        clock.tick,
                        &mut commands,
                    );
                    continue;
                }
                let operation = commands
                    .spawn((
                        *member,
                        BehaviorTransactionPayment {
                            actor,
                            facility: pending.facility,
                            transaction: transaction.name,
                        },
                    ))
                    .id();
                if let Some(current_job) = keeper_refill {
                    commands.entity(operation).insert(
                        super::keeper_refill_settlement::StagedBehaviorRefill {
                            job: current_job.job(),
                            definition: object.expect("validated refill object").0.id,
                            continuation: running
                                .keeper_refill
                                .expect("validated refill continuation"),
                        },
                    );
                }
                let owner = if running.general || transaction.track_on_parent {
                    Account::Zoo
                } else if let Some((tracker, _)) = tracker {
                    Account::Entity(tracker)
                } else {
                    Account::External
                };
                let (debit, credit) = if keeper_refill.is_some() {
                    (Account::Zoo, Account::External)
                } else if (transaction.kind == EconomyTransactionKind::Debit) != reversed {
                    (Account::Entity(account_recipient), owner)
                } else {
                    (owner, Account::Entity(account_recipient))
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
                let user_tracker = tracker.map_or(account_recipient, |(tracker, _)| tracker);
                apply_usage_change(
                    user_tracker,
                    count,
                    adding,
                    &mut usage_counters,
                    &mut new_usage,
                );
                if transaction.track_on_parent && pending.facility != user_tracker {
                    apply_usage_change(
                        pending.facility,
                        count,
                        adding,
                        &mut usage_counters,
                        &mut new_usage,
                    );
                }
                finish_transaction_step(
                    actor,
                    instruction,
                    &mut pending,
                    transaction.next_transaction,
                    clock.tick,
                    &mut commands,
                );
            }
            EconomyTransactionKind::SetCash => {
                let recipient = tracker.map_or(account_recipient, |(tracker, _)| tracker);
                let Some(amount) = authored_currency_to_money(amount) else {
                    fail_transaction(
                        actor,
                        transaction.name,
                        "invalid account balance",
                        &mut commands,
                    );
                    continue;
                };
                if let Ok(mut wallet) = wallets.get_mut(recipient) {
                    if wallet.0 != amount {
                        wallet.0 = amount;
                    }
                } else {
                    commands.entity(recipient).try_insert(Wallet(amount));
                }
                finish_transaction_step(
                    actor,
                    instruction,
                    &mut pending,
                    transaction.next_transaction,
                    clock.tick,
                    &mut commands,
                );
            }
        }
    }
    for (entity, usage) in new_usage {
        commands.entity(entity).try_insert(usage);
    }
}

fn apply_usage_change(
    entity: Entity,
    count: i32,
    adding: bool,
    usage_counters: &mut Query<&mut EconomyUsage>,
    new_usage: &mut HashMap<Entity, EconomyUsage>,
) {
    if count != 0 {
        if let Ok(mut usage) = usage_counters.get_mut(entity) {
            usage.apply_user_change(count, adding);
        } else {
            new_usage
                .entry(entity)
                .or_default()
                .apply_user_change(count, adding);
        }
    }
}

fn fail_transaction(actor: Entity, transaction: AssetId, reason: &str, commands: &mut Commands) {
    warn!(
        ?actor,
        ?transaction,
        reason,
        "authored economy action failed"
    );
    commands
        .entity(actor)
        .remove::<RunningBehaviorEconomyTransaction>();
    mark_behavior_task_for_failure_and_stop_navigation(actor, commands);
}

fn finish_transaction_step(
    actor: Entity,
    instruction: &RunningBehaviorInstruction,
    pending: &mut RunningBehaviorEconomyTransaction,
    next: Option<AssetId>,
    tick: u64,
    commands: &mut Commands,
) {
    if let Some(next) = next {
        pending.transaction = next;
        pending.payment = None;
        // Native dispatch passes the supplied override to each chained transaction.
    } else {
        commands
            .entity(actor)
            .remove::<RunningBehaviorEconomyTransaction>();
        instruction.complete(actor, tick, commands);
    }
}

pub(super) fn finish_settled_behavior_transactions(
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    operations: Query<(
        &BehaviorTransactionPayment,
        &WorldMember,
        Has<super::keeper_refill_settlement::StagedBehaviorRefill>,
    )>,
    mut tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &mut RunningBehaviorEconomyTransaction,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for payment in completed.read() {
        let Ok((owner, member, refill)) = operations.get(payment.operation) else {
            continue;
        };
        if let Ok((task, instruction, mut pending)) = tasks.get_mut(owner.actor) {
            if pending.payment == Some(payment.operation) {
                let next = pending.next_transaction;
                if let Some(clock) = clocks
                    .get(member.root)
                    .ok()
                    .filter(|_| !refill && pending.owns(task, instruction))
                {
                    finish_transaction_step(
                        owner.actor,
                        instruction,
                        &mut pending,
                        next,
                        clock.tick,
                        &mut commands,
                    );
                } else {
                    commands
                        .entity(owner.actor)
                        .remove::<RunningBehaviorEconomyTransaction>();
                }
            }
        }
        commands.entity(payment.operation).despawn();
    }
    for payment in rejected.read() {
        let Ok((owner, _, _)) = operations.get(payment.operation) else {
            continue;
        };
        if let Ok((task, instruction, pending)) = tasks.get_mut(owner.actor) {
            if pending.payment == Some(payment.operation) {
                if pending.owns(task, instruction) {
                    fail_transaction(
                        owner.actor,
                        pending.transaction,
                        "account settlement rejected payment",
                        &mut commands,
                    );
                } else {
                    commands
                        .entity(owner.actor)
                        .remove::<RunningBehaviorEconomyTransaction>();
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
            With<RunningBehaviorEconomyTransaction>,
            Or<(
                Without<RunBehaviorEconomy>,
                With<PendingBehaviorTaskFailure>,
            )>,
        ),
    >,
) {
    for actor in &interrupted {
        commands
            .entity(actor)
            .remove::<RunningBehaviorEconomyTransaction>();
    }
}
