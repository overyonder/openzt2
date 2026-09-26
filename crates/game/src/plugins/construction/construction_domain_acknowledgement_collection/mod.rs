use bevy::prelude::*;

use crate::plugins::{
    placement::placement_transaction_types::ObjectPlacementEditApplicationAcknowledged,
    shows::show_platform_upgrade_types::ShowPlatformUpgradeEditAcknowledged,
    terrain::terrain_edit_types::TerrainEditAcknowledged,
    topology::topology_edit_types::TopologyEditAcknowledged,
    transport_tours::transport_track_construction_types::TransportTrackConstructionEditApplicationAcknowledged,
};

use super::construction_application_failure_recovery::{
    begin_construction_application_rollback, construction_domain_status_before_application,
    finish_failed_construction_application,
};
use super::construction_edit_history_types::{
    ConstructionEditDirection, ConstructionTransactionApplied, RecordAppliedConstructionTransaction,
};
use super::construction_transaction_types::{
    ConstructionCommitFailed, ConstructionCommitFailure, ConstructionCommitted,
    ConstructionFeedbackOrigin, DomainAckStatus, EditApplication, EditCommitPhase,
    EditCommitProgress, EditTransaction, RollbackContext,
};

#[derive(Clone, Copy)]
enum ConstructionApplicationDomain {
    Terrain,
    Topology,
    Placement,
    Shows,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_construction_domain_application_acknowledgements(
    mut commands: Commands,
    mut terrain_acknowledgements: MessageReader<TerrainEditAcknowledged>,
    mut topology_acknowledgements: MessageReader<TopologyEditAcknowledged>,
    mut object_placement_acknowledgements: MessageReader<
        ObjectPlacementEditApplicationAcknowledged,
    >,
    mut show_platform_upgrade_acknowledgements: MessageReader<ShowPlatformUpgradeEditAcknowledged>,
    mut transport_track_acknowledgements: MessageReader<
        TransportTrackConstructionEditApplicationAcknowledged,
    >,
    mut construction_transactions: Query<(
        Entity,
        Option<&EditTransaction>,
        &mut EditCommitProgress,
        Option<&RollbackContext>,
        Option<&ConstructionFeedbackOrigin>,
    )>,
    mut transaction_record_requests: MessageWriter<RecordAppliedConstructionTransaction>,
    mut applied_transactions: MessageWriter<ConstructionTransactionApplied>,
    mut failed_construction_commits: MessageWriter<ConstructionCommitFailed>,
    mut committed_construction_transactions: MessageWriter<ConstructionCommitted>,
) {
    for acknowledgement in terrain_acknowledgements.read() {
        accept_construction_domain_application_acknowledgement(
            &mut construction_transactions,
            acknowledgement.transaction,
            acknowledgement.application,
            acknowledgement.accepted,
            ConstructionApplicationDomain::Terrain,
        );
    }
    for acknowledgement in topology_acknowledgements.read() {
        accept_construction_domain_application_acknowledgement(
            &mut construction_transactions,
            acknowledgement.transaction,
            acknowledgement.application,
            acknowledgement.accepted,
            ConstructionApplicationDomain::Topology,
        );
    }
    for acknowledgement in object_placement_acknowledgements.read() {
        accept_construction_domain_application_acknowledgement(
            &mut construction_transactions,
            acknowledgement.transaction,
            acknowledgement.application,
            acknowledgement.accepted,
            ConstructionApplicationDomain::Placement,
        );
    }
    for acknowledgement in show_platform_upgrade_acknowledgements.read() {
        accept_construction_domain_application_acknowledgement(
            &mut construction_transactions,
            acknowledgement.transaction,
            acknowledgement.application,
            acknowledgement.accepted,
            ConstructionApplicationDomain::Shows,
        );
    }
    for acknowledgement in transport_track_acknowledgements.read() {
        accept_construction_domain_application_acknowledgement(
            &mut construction_transactions,
            acknowledgement.transaction,
            acknowledgement.application,
            acknowledgement.accepted,
            ConstructionApplicationDomain::Topology,
        );
    }

    for (transaction_entity, transaction, mut progress, rollback_context, feedback_origin) in
        &mut construction_transactions
    {
        match progress.phase {
            EditCommitPhase::Applying
                if construction_application_has_rejected_domain(&progress) =>
            {
                let failed_application = progress.application;
                begin_construction_application_rollback(
                    &mut commands,
                    transaction_entity,
                    &mut progress,
                    failed_application,
                    ConstructionCommitFailure::DomainRejected,
                );
            }
            EditCommitPhase::Applying
                if progress.all_expected_are(
                    construction_domain_status_after_successful_application(progress.application),
                ) =>
            {
                match progress.application {
                    EditApplication::InitialCommit => {
                        progress.phase = EditCommitPhase::Complete;
                        if let (Some(transaction), Some(feedback_origin)) =
                            (transaction, feedback_origin)
                        {
                            committed_construction_transactions.write(ConstructionCommitted {
                                transaction: transaction_entity,
                                cost: transaction.cost,
                                screen_position: feedback_origin.0,
                            });
                        }
                        transaction_record_requests
                            .write(RecordAppliedConstructionTransaction(transaction_entity));
                    }
                    EditApplication::Redo => {
                        progress.phase = EditCommitPhase::Complete;
                        applied_transactions.write(ConstructionTransactionApplied {
                            transaction: transaction_entity,
                            direction: ConstructionEditDirection::Redo,
                            accepted: true,
                        });
                    }
                    EditApplication::Undo => {
                        progress.phase = EditCommitPhase::AwaitingCompensation;
                    }
                    EditApplication::FailureRollback => {}
                }
            }
            EditCommitPhase::RollingBack => {
                let Some(rollback_context) = rollback_context else {
                    continue;
                };
                if construction_rollback_is_complete(&progress, rollback_context.failed_application)
                {
                    if matches!(
                        rollback_context.failed_application,
                        EditApplication::InitialCommit | EditApplication::Redo
                    ) && progress.cost.0 != 0
                    {
                        progress.phase = EditCommitPhase::AwaitingCompensation;
                    } else {
                        failed_construction_commits.write(ConstructionCommitFailed {
                            transaction: transaction_entity,
                            reason: rollback_context.failure,
                        });
                        finish_failed_construction_application(
                            &mut commands,
                            &mut progress,
                            transaction_entity,
                            rollback_context.failed_application,
                            &mut applied_transactions,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn accept_construction_domain_application_acknowledgement(
    construction_transactions: &mut Query<(
        Entity,
        Option<&EditTransaction>,
        &mut EditCommitProgress,
        Option<&RollbackContext>,
        Option<&ConstructionFeedbackOrigin>,
    )>,
    transaction_entity: Entity,
    application: EditApplication,
    accepted: bool,
    application_domain: ConstructionApplicationDomain,
) {
    let Ok((_, _, mut progress, rollback_context, _)) =
        construction_transactions.get_mut(transaction_entity)
    else {
        return;
    };
    if application != progress.application {
        return;
    }
    let domain_status =
        construction_application_domain_status_mut(&mut progress, application_domain);
    if application == EditApplication::FailureRollback {
        let Some(rollback_context) = rollback_context else {
            return;
        };
        let domain_was_changed = match rollback_context.failed_application {
            EditApplication::Undo => *domain_status == DomainAckStatus::Reverted,
            _ => *domain_status == DomainAckStatus::Applied,
        };
        if !domain_was_changed {
            return;
        }
        if accepted {
            *domain_status =
                construction_domain_status_before_application(rollback_context.failed_application);
        }
    } else if *domain_status == DomainAckStatus::Pending {
        *domain_status = if accepted {
            construction_domain_status_after_successful_application(application)
        } else {
            DomainAckStatus::Rejected
        };
    }
}

fn construction_application_domain_status_mut(
    progress: &mut EditCommitProgress,
    application_domain: ConstructionApplicationDomain,
) -> &mut DomainAckStatus {
    match application_domain {
        ConstructionApplicationDomain::Terrain => &mut progress.terrain,
        ConstructionApplicationDomain::Topology => &mut progress.topology,
        ConstructionApplicationDomain::Placement => &mut progress.placement,
        ConstructionApplicationDomain::Shows => &mut progress.shows,
    }
}

fn construction_application_has_rejected_domain(progress: &EditCommitProgress) -> bool {
    [
        progress.terrain,
        progress.topology,
        progress.placement,
        progress.shows,
    ]
    .contains(&DomainAckStatus::Rejected)
}

fn construction_domain_status_after_successful_application(
    application: EditApplication,
) -> DomainAckStatus {
    match application {
        EditApplication::Undo | EditApplication::FailureRollback => DomainAckStatus::Reverted,
        EditApplication::InitialCommit | EditApplication::Redo => DomainAckStatus::Applied,
    }
}

fn construction_rollback_is_complete(
    progress: &EditCommitProgress,
    failed_application: EditApplication,
) -> bool {
    progress.all_expected_are(construction_domain_status_before_application(
        failed_application,
    ))
}
