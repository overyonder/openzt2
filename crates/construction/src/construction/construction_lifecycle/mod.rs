mod domains;
#[cfg(test)]
mod tests;

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    construction::{
        construction_edit_history_types::*, construction_transaction_types::*,
        ConstructionDomainAcknowledgementCollectionSet, ConstructionDomainPreparationCollectionSet,
        FixedConstructionSet,
    },
    economy::{
        account_transaction_types::*, general_economy_types::GeneralEconomyTransaction,
        money_types::Money,
    },
};

use super::{
    construction_history::ConstructionHistory,
    construction_payment_operation_types::ConstructionPaymentPurpose,
    construction_payment_request_creation::create_construction_payment_request,
};
use domains::{settlement_facts, ApplicationResults, Domains};

pub use domains::PreparedDomain;

/// Present only while a transaction can still change the world or its accounts.
#[derive(Component, Default)]
pub struct ConstructionActive;

#[derive(Component)]
#[require(ConstructionActive)]
pub struct Preparing;

#[derive(Component)]
struct ApplyEdit(EditApplication);

#[derive(Component)]
struct InitialTransfer;

#[derive(Component)]
struct Compensate;

#[derive(Component)]
struct AwaitPayment;

#[derive(SystemParam)]
struct Settlement<'w, 's> {
    records: MessageWriter<'w, RecordAppliedConstructionTransaction>,
    completions: MessageWriter<'w, ConstructionCommitted>,
    history: MessageWriter<'w, ConstructionTransactionApplied>,
    failures: MessageWriter<'w, ConstructionCommitFailed>,
    feedback: Query<'w, 's, &'static ConstructionFeedbackOrigin>,
}

impl Settlement<'_, '_> {
    fn success(
        &mut self,
        commands: &mut Commands,
        entity: Entity,
        application: EditApplication,
        cost: Money,
    ) {
        if application == EditApplication::InitialCommit {
            if let Ok(origin) = self.feedback.get(entity) {
                self.completions.write(ConstructionCommitted {
                    transaction: entity,
                    cost,
                    screen_position: origin.0,
                });
            }
            self.records
                .write(RecordAppliedConstructionTransaction(entity));
        } else if let Some(direction) = history_direction(application) {
            self.history.write(ConstructionTransactionApplied {
                transaction: entity,
                direction,
                accepted: true,
            });
        }
        clear_active_edit(commands, entity);
    }

    fn failure(
        &mut self,
        commands: &mut Commands,
        entity: Entity,
        application: EditApplication,
        reason: ConstructionCommitFailure,
    ) {
        self.failures.write(ConstructionCommitFailed {
            transaction: entity,
            reason,
        });
        if let Some(direction) = history_direction(application) {
            self.history.write(ConstructionTransactionApplied {
                transaction: entity,
                direction,
                accepted: false,
            });
            clear_active_edit(commands, entity);
        } else {
            commands.entity(entity).try_despawn();
        }
    }
}

fn history_direction(application: EditApplication) -> Option<ConstructionEditDirection> {
    match application {
        EditApplication::Undo => Some(ConstructionEditDirection::Undo),
        EditApplication::Redo => Some(ConstructionEditDirection::Redo),
        EditApplication::InitialCommit | EditApplication::FailureRollback => None,
    }
}

fn clear_active_edit(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(
        ConstructionActive,
        ApplyEdit,
        RollbackContext,
        AwaitPayment,
        Compensate,
        ApplicationResults,
    )>();
}

fn finalize_preparation(
    mut commands: Commands,
    mut edits: Query<(Entity, &mut EditTransaction, Domains), With<Preparing>>,
    mut settlement: Settlement,
) {
    for (entity, mut edit, domains) in &mut edits {
        let facts = settlement_facts(domains);
        let cost = facts.iter().try_fold(0_i64, |total, domain| {
            total.checked_add(domain.cost.map_or(0, |cost| cost.0))
        });
        let failure = facts
            .iter()
            .find_map(|domain| domain.rejection)
            .map(ConstructionCommitFailure::Preparation)
            .or_else(|| {
                cost.filter(|cost| cost.checked_abs().is_some())
                    .is_none()
                    .then_some(ConstructionCommitFailure::Economy(
                        TransactionRejection::Overflow,
                    ))
            })
            .or_else(|| {
                facts
                    .iter()
                    .all(|domain| domain.cost.is_none())
                    .then_some(ConstructionCommitFailure::NoDomain)
            });
        if let Some(reason) = failure {
            settlement.failure(
                &mut commands,
                entity,
                EditApplication::InitialCommit,
                reason,
            );
        } else if let Some(cost) = cost {
            edit.cost = Money(cost);
            commands
                .entity(entity)
                .remove::<Preparing>()
                .insert(InitialTransfer);
        }
    }
}

fn initiate_transfers(
    mut commands: Commands,
    edits: Query<
        (
            Entity,
            &EditTransaction,
            &WorldMember,
            Option<&GeneralEconomyTransaction>,
        ),
        With<InitialTransfer>,
    >,
    mut payments: MessageWriter<TransactionRequest>,
    mut settlement: Settlement,
) {
    for (entity, edit, member, general) in &edits {
        commands.entity(entity).remove::<InitialTransfer>();
        if edit.cost.0 == 0 {
            commands
                .entity(entity)
                .insert(ApplyEdit(EditApplication::InitialCommit));
        } else if let Some(request) = create_construction_payment_request(
            &mut commands,
            entity,
            *member,
            edit.cost,
            ConstructionPaymentPurpose::InitialTransfer,
            false,
            general.copied(),
        ) {
            commands.entity(entity).insert(AwaitPayment);
            payments.write(request);
        } else {
            settlement.failure(
                &mut commands,
                entity,
                EditApplication::InitialCommit,
                ConstructionCommitFailure::Economy(TransactionRejection::Overflow),
            );
        }
    }
}

fn begin_history_application(
    mut commands: Commands,
    mut requests: MessageReader<ApplyConstructionTransaction>,
    history: ConstructionHistory,
    busy: Query<(), With<ConstructionActive>>,
    edits: Query<(
        &EditTransaction,
        &WorldMember,
        Option<&GeneralEconomyTransaction>,
    )>,
    mut payments: MessageWriter<TransactionRequest>,
    mut settlement: Settlement,
) {
    if !busy.is_empty() {
        requests.clear();
        return;
    }
    for request in requests.read() {
        if history.next(request.direction) != Some(request.transaction) {
            continue;
        }
        let entity = request.transaction;
        let Ok((edit, member, general)) = edits.get(entity) else {
            continue;
        };
        let application = match request.direction {
            ConstructionEditDirection::Undo => EditApplication::Undo,
            ConstructionEditDirection::Redo => EditApplication::Redo,
        };
        if application == EditApplication::Undo || edit.cost.0 == 0 {
            commands
                .entity(entity)
                .insert((ConstructionActive, ApplyEdit(application)));
        } else if let Some(payment) = create_construction_payment_request(
            &mut commands,
            entity,
            *member,
            edit.cost,
            ConstructionPaymentPurpose::RedoTransfer,
            false,
            general.copied(),
        ) {
            commands
                .entity(entity)
                .insert((ConstructionActive, AwaitPayment));
            payments.write(payment);
        } else {
            settlement.failure(
                &mut commands,
                entity,
                application,
                ConstructionCommitFailure::Economy(TransactionRejection::Overflow),
            );
        }
        break;
    }
}

fn settle_domain_applications(
    mut commands: Commands,
    edits: Query<(Entity, &EditTransaction, &ApplyEdit, Domains)>,
    mut settlement: Settlement,
) {
    for (entity, edit, apply, domains) in &edits {
        let participants = settlement_facts(domains);
        let mut applied = participants
            .iter()
            .filter(|domain| domain.cost.is_some())
            .peekable();
        if applied.peek().is_none() || applied.clone().any(|result| result.accepted.is_none()) {
            continue;
        }
        if applied.any(|domain| domain.accepted == Some(false)) {
            start_rollback(
                &mut commands,
                entity,
                apply.0,
                ConstructionCommitFailure::DomainRejected,
            );
        } else if apply.0 == EditApplication::Undo {
            commands
                .entity(entity)
                .remove::<ApplyEdit>()
                .insert(Compensate);
        } else {
            settlement.success(&mut commands, entity, apply.0, edit.cost);
        }
    }
}

fn start_rollback(
    commands: &mut Commands,
    entity: Entity,
    failed_application: EditApplication,
    failure: ConstructionCommitFailure,
) {
    commands
        .entity(entity)
        .remove::<(ApplyEdit, AwaitPayment, Compensate)>()
        .insert(RollbackContext {
            failed_application,
            failure,
        });
}

fn settle_domain_rollbacks(
    mut commands: Commands,
    edits: Query<
        (Entity, &EditTransaction, &RollbackContext, Domains),
        (Without<Compensate>, Without<AwaitPayment>),
    >,
    mut settlement: Settlement,
) {
    for (entity, edit, rollback, domains) in &edits {
        let facts = settlement_facts(domains);
        let mut applied = facts.iter().filter(|domain| domain.accepted == Some(true));
        if applied.clone().any(|domain| domain.reverted.is_none()) {
            continue;
        }
        if applied.any(|domain| domain.reverted == Some(false)) {
            error!(
                ?entity,
                "a construction domain could not restore its prior state"
            );
        }
        if rollback.failed_application == EditApplication::Undo || edit.cost.0 == 0 {
            settlement.failure(
                &mut commands,
                entity,
                rollback.failed_application,
                rollback.failure,
            );
        } else {
            commands.entity(entity).insert(Compensate);
        }
    }
}

fn request_compensation(
    mut commands: Commands,
    edits: Query<
        (
            Entity,
            &EditTransaction,
            &WorldMember,
            Option<&RollbackContext>,
            Option<&GeneralEconomyTransaction>,
        ),
        With<Compensate>,
    >,
    mut payments: MessageWriter<TransactionRequest>,
    mut settlement: Settlement,
) {
    for (entity, edit, member, rollback, general) in &edits {
        commands.entity(entity).remove::<Compensate>();
        let purpose = if rollback.is_some() {
            ConstructionPaymentPurpose::FailureCompensation
        } else {
            ConstructionPaymentPurpose::UndoCompensation
        };
        if edit.cost.0 == 0 {
            settlement.success(&mut commands, entity, EditApplication::Undo, edit.cost);
        } else if let Some(request) = create_construction_payment_request(
            &mut commands,
            entity,
            *member,
            edit.cost,
            purpose,
            true,
            general.copied(),
        ) {
            commands.entity(entity).insert(AwaitPayment);
            payments.write(request);
        } else {
            let reason = ConstructionCommitFailure::Compensation(TransactionRejection::Overflow);
            if let Some(rollback) = rollback {
                settlement.failure(&mut commands, entity, rollback.failed_application, reason);
            } else {
                start_rollback(&mut commands, entity, EditApplication::Undo, reason);
            }
        }
    }
}

fn settle_payments(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    operations: Query<(&ChildOf, &ConstructionPaymentPurpose)>,
    awaiting: Query<(&EditTransaction, Option<&RollbackContext>), With<AwaitPayment>>,
    mut settlement: Settlement,
) {
    let mut received = bevy::ecs::entity::EntityHashSet::new();
    let results = completed
        .read()
        .map(|result| (result.operation, Ok(())))
        .chain(
            rejected
                .read()
                .map(|result| (result.operation, Err(result.reason))),
        );
    for (operation, result) in results {
        let Ok((parent, purpose)) = operations.get(operation) else {
            continue;
        };
        if !received.insert(operation) {
            continue;
        }
        commands.entity(operation).try_despawn();
        let entity = parent.parent();
        let Ok((edit, rollback)) = awaiting.get(entity) else {
            continue;
        };
        commands.entity(entity).remove::<AwaitPayment>();
        match (purpose, result) {
            (
                ConstructionPaymentPurpose::InitialTransfer
                | ConstructionPaymentPurpose::RedoTransfer,
                result,
            ) => {
                let application = if *purpose == ConstructionPaymentPurpose::InitialTransfer {
                    EditApplication::InitialCommit
                } else {
                    EditApplication::Redo
                };
                match result {
                    Ok(()) => {
                        commands.entity(entity).insert(ApplyEdit(application));
                    }
                    Err(reason) => settlement.failure(
                        &mut commands,
                        entity,
                        application,
                        ConstructionCommitFailure::Economy(reason),
                    ),
                }
            }
            (ConstructionPaymentPurpose::UndoCompensation, Ok(())) => {
                settlement.success(&mut commands, entity, EditApplication::Undo, edit.cost);
            }
            (ConstructionPaymentPurpose::UndoCompensation, Err(reason)) => {
                start_rollback(
                    &mut commands,
                    entity,
                    EditApplication::Undo,
                    ConstructionCommitFailure::Compensation(reason),
                );
            }
            (ConstructionPaymentPurpose::FailureCompensation, result) => {
                if let Some(rollback) = rollback {
                    let reason = result
                        .err()
                        .map_or(rollback.failure, ConstructionCommitFailure::Compensation);
                    settlement.failure(&mut commands, entity, rollback.failed_application, reason);
                }
            }
        }
    }
}

pub(super) fn install(app: &mut App) {
    domains::install(app);
    app.add_systems(
        FixedUpdate,
        finalize_preparation
            .after(ConstructionDomainPreparationCollectionSet)
            .in_set(FixedConstructionSet::Prepare),
    )
    .add_systems(
        FixedUpdate,
        (initiate_transfers, request_compensation).in_set(FixedConstructionSet::RequestEconomy),
    )
    .add_systems(
        FixedUpdate,
        settle_payments.in_set(FixedConstructionSet::CompleteEconomy),
    )
    .add_systems(
        FixedUpdate,
        begin_history_application
            .after(settle_payments)
            .in_set(FixedConstructionSet::CompleteEconomy),
    )
    .add_systems(
        FixedUpdate,
        (settle_domain_applications, settle_domain_rollbacks)
            .after(ConstructionDomainAcknowledgementCollectionSet)
            .before(super::construction_history::retain_accepted_edits)
            .before(super::construction_history::record_history_application)
            .in_set(FixedConstructionSet::Cleanup),
    );
}
