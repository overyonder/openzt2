use bevy::prelude::*;

use super::construction_edit_history_types::{
    ConstructionEditDirection, ConstructionTransactionApplied,
};
use super::construction_transaction_types::{
    CompensationRequested, ConstructionCommitFailure, DomainAckStatus, EditApplication,
    EditCommitPhase, EditCommitProgress, RollbackContext, RollbackDispatched,
};

pub(super) fn construction_domain_status_after_rejected_redo_payment(
    domain_status: DomainAckStatus,
) -> DomainAckStatus {
    if domain_status == DomainAckStatus::NotExpected {
        domain_status
    } else {
        DomainAckStatus::Reverted
    }
}

pub(super) fn begin_construction_application_rollback(
    commands: &mut Commands,
    transaction_entity: Entity,
    progress: &mut EditCommitProgress,
    failed_application: EditApplication,
    failure: ConstructionCommitFailure,
) {
    let domain_status_before_failed_application =
        construction_domain_status_before_application(failed_application);
    for domain_status in [
        &mut progress.terrain,
        &mut progress.topology,
        &mut progress.placement,
        &mut progress.shows,
    ] {
        if matches!(
            *domain_status,
            DomainAckStatus::Pending | DomainAckStatus::Rejected
        ) {
            *domain_status = domain_status_before_failed_application;
        }
    }
    progress.phase = EditCommitPhase::RollingBack;
    progress.application = EditApplication::FailureRollback;
    commands.entity(transaction_entity).insert(RollbackContext {
        failed_application,
        failure,
    });
    commands
        .entity(transaction_entity)
        .remove::<RollbackDispatched>();
}

pub(super) fn finish_failed_construction_application(
    commands: &mut Commands,
    progress: &mut EditCommitProgress,
    transaction_entity: Entity,
    failed_application: EditApplication,
    applied_transactions: &mut MessageWriter<ConstructionTransactionApplied>,
) {
    match failed_application {
        EditApplication::InitialCommit => {
            progress.phase = EditCommitPhase::Failed;
            commands.entity(transaction_entity).despawn();
        }
        EditApplication::Undo | EditApplication::Redo => {
            progress.phase = EditCommitPhase::Complete;
            progress.application = failed_application;
            applied_transactions.write(ConstructionTransactionApplied {
                transaction: transaction_entity,
                direction: if failed_application == EditApplication::Undo {
                    ConstructionEditDirection::Undo
                } else {
                    ConstructionEditDirection::Redo
                },
                accepted: false,
            });
            commands
                .entity(transaction_entity)
                .remove::<(RollbackContext, CompensationRequested)>();
        }
        EditApplication::FailureRollback => {}
    }
}

pub(super) fn construction_domain_status_before_application(
    application: EditApplication,
) -> DomainAckStatus {
    if application == EditApplication::Undo {
        DomainAckStatus::Applied
    } else {
        DomainAckStatus::Reverted
    }
}
