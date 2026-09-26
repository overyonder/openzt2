use bevy::prelude::*;

use crate::plugins::{
    economy::{account_transaction_types::TransactionRejection, money_types::Money},
    placement::placement_transaction_types::{
        ObjectPlacementEditPreparationRejected, ObjectPlacementEditPrepared,
    },
    terrain::terrain_edit_types::{TerrainEditPreparationRejected, TerrainEditPrepared},
    topology::topology_edit_types::{TopologyEditPreparationRejected, TopologyEditPrepared},
    transport_tours::transport_track_construction_types::{
        TransportTrackConstructionEditPreparationRejected, TransportTrackConstructionEditPrepared,
    },
};

use super::construction_interaction_types::PlacementFailure;
use super::construction_transaction_types::{
    ConstructionCommitFailed, ConstructionCommitFailure, DomainAckStatus, EditCommitPhase,
    EditCommitProgress, EditTransaction,
};

#[derive(Clone, Copy)]
enum ConstructionPreparationDomain {
    Terrain,
    Topology,
    Placement,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_construction_domain_preparation_results(
    mut commands: Commands,
    mut prepared_terrain_edits: MessageReader<TerrainEditPrepared>,
    mut rejected_terrain_edits: MessageReader<TerrainEditPreparationRejected>,
    mut prepared_topology_edits: MessageReader<TopologyEditPrepared>,
    mut rejected_topology_edits: MessageReader<TopologyEditPreparationRejected>,
    mut prepared_object_placement_edits: MessageReader<ObjectPlacementEditPrepared>,
    mut rejected_object_placement_edits: MessageReader<ObjectPlacementEditPreparationRejected>,
    mut prepared_transport_track_edits: MessageReader<TransportTrackConstructionEditPrepared>,
    mut rejected_transport_track_edits: MessageReader<
        TransportTrackConstructionEditPreparationRejected,
    >,
    mut construction_transactions: Query<(Entity, &mut EditCommitProgress, &mut EditTransaction)>,
    mut failed_construction_commits: MessageWriter<ConstructionCommitFailed>,
) {
    for prepared_edit in prepared_terrain_edits.read() {
        if accept_construction_domain_preparation(
            &mut construction_transactions,
            prepared_edit.transaction,
            prepared_edit.cost,
            ConstructionPreparationDomain::Terrain,
        )
        .is_err()
        {
            reject_construction_preparation_cost_overflow(
                &mut commands,
                &mut construction_transactions,
                &mut failed_construction_commits,
                prepared_edit.transaction,
            );
        }
    }
    for prepared_edit in prepared_topology_edits.read() {
        if accept_construction_domain_preparation(
            &mut construction_transactions,
            prepared_edit.transaction,
            prepared_edit.cost,
            ConstructionPreparationDomain::Topology,
        )
        .is_err()
        {
            reject_construction_preparation_cost_overflow(
                &mut commands,
                &mut construction_transactions,
                &mut failed_construction_commits,
                prepared_edit.transaction,
            );
        }
    }
    for prepared_edit in prepared_object_placement_edits.read() {
        if accept_construction_domain_preparation(
            &mut construction_transactions,
            prepared_edit.transaction,
            prepared_edit.cost,
            ConstructionPreparationDomain::Placement,
        )
        .is_err()
        {
            reject_construction_preparation_cost_overflow(
                &mut commands,
                &mut construction_transactions,
                &mut failed_construction_commits,
                prepared_edit.transaction,
            );
        }
    }
    for prepared_edit in prepared_transport_track_edits.read() {
        if accept_construction_domain_preparation(
            &mut construction_transactions,
            prepared_edit.transaction,
            prepared_edit.cost,
            ConstructionPreparationDomain::Topology,
        )
        .is_err()
        {
            reject_construction_preparation_cost_overflow(
                &mut commands,
                &mut construction_transactions,
                &mut failed_construction_commits,
                prepared_edit.transaction,
            );
        }
    }
    for rejected_edit in rejected_terrain_edits.read() {
        reject_construction_domain_preparation(
            &mut commands,
            &mut construction_transactions,
            &mut failed_construction_commits,
            rejected_edit.transaction,
            rejected_edit.reason,
        );
    }
    for rejected_edit in rejected_topology_edits.read() {
        reject_construction_domain_preparation(
            &mut commands,
            &mut construction_transactions,
            &mut failed_construction_commits,
            rejected_edit.transaction,
            rejected_edit.reason,
        );
    }
    for rejected_edit in rejected_object_placement_edits.read() {
        reject_construction_domain_preparation(
            &mut commands,
            &mut construction_transactions,
            &mut failed_construction_commits,
            rejected_edit.transaction,
            rejected_edit.reason,
        );
    }
    for rejected_edit in rejected_transport_track_edits.read() {
        reject_construction_domain_preparation(
            &mut commands,
            &mut construction_transactions,
            &mut failed_construction_commits,
            rejected_edit.transaction,
            rejected_edit.reason,
        );
    }

    for (transaction_entity, mut progress, mut transaction) in &mut construction_transactions {
        if progress.phase != EditCommitPhase::Preparing {
            continue;
        }
        if !progress.expects_domain() {
            progress.phase = EditCommitPhase::Failed;
            failed_construction_commits.write(ConstructionCommitFailed {
                transaction: transaction_entity,
                reason: ConstructionCommitFailure::NoDomain,
            });
            commands.entity(transaction_entity).despawn();
            continue;
        }
        transaction.cost = progress.cost;
        progress.phase = EditCommitPhase::AwaitingEconomy;
    }
}

fn accept_construction_domain_preparation(
    construction_transactions: &mut Query<(Entity, &mut EditCommitProgress, &mut EditTransaction)>,
    transaction_entity: Entity,
    domain_cost: Money,
    preparation_domain: ConstructionPreparationDomain,
) -> Result<(), ()> {
    let Ok((_, mut progress, _)) = construction_transactions.get_mut(transaction_entity) else {
        return Ok(());
    };
    if progress.phase != EditCommitPhase::Preparing
        || construction_preparation_domain_status(&progress, preparation_domain)
            != DomainAckStatus::NotExpected
    {
        return Ok(());
    }
    let total_cost = progress.cost.0.checked_add(domain_cost.0).ok_or(())?;
    progress.cost = Money(total_cost);
    *construction_preparation_domain_status_mut(&mut progress, preparation_domain) =
        DomainAckStatus::Pending;
    Ok(())
}

fn reject_construction_preparation_cost_overflow(
    commands: &mut Commands,
    construction_transactions: &mut Query<(Entity, &mut EditCommitProgress, &mut EditTransaction)>,
    failed_construction_commits: &mut MessageWriter<ConstructionCommitFailed>,
    transaction_entity: Entity,
) {
    let Ok((_, mut progress, _)) = construction_transactions.get_mut(transaction_entity) else {
        return;
    };
    progress.phase = EditCommitPhase::Failed;
    failed_construction_commits.write(ConstructionCommitFailed {
        transaction: transaction_entity,
        reason: ConstructionCommitFailure::Economy(TransactionRejection::Overflow),
    });
    commands.entity(transaction_entity).despawn();
}

fn reject_construction_domain_preparation(
    commands: &mut Commands,
    construction_transactions: &mut Query<(Entity, &mut EditCommitProgress, &mut EditTransaction)>,
    failed_construction_commits: &mut MessageWriter<ConstructionCommitFailed>,
    transaction_entity: Entity,
    failure_reason: PlacementFailure,
) {
    let Ok((_, mut progress, _)) = construction_transactions.get_mut(transaction_entity) else {
        return;
    };
    if progress.phase != EditCommitPhase::Preparing {
        return;
    }
    progress.phase = EditCommitPhase::Failed;
    failed_construction_commits.write(ConstructionCommitFailed {
        transaction: transaction_entity,
        reason: ConstructionCommitFailure::Preparation(failure_reason),
    });
    commands.entity(transaction_entity).despawn();
}

fn construction_preparation_domain_status(
    progress: &EditCommitProgress,
    preparation_domain: ConstructionPreparationDomain,
) -> DomainAckStatus {
    match preparation_domain {
        ConstructionPreparationDomain::Terrain => progress.terrain,
        ConstructionPreparationDomain::Topology => progress.topology,
        ConstructionPreparationDomain::Placement => progress.placement,
    }
}

fn construction_preparation_domain_status_mut(
    progress: &mut EditCommitProgress,
    preparation_domain: ConstructionPreparationDomain,
) -> &mut DomainAckStatus {
    match preparation_domain {
        ConstructionPreparationDomain::Terrain => &mut progress.terrain,
        ConstructionPreparationDomain::Topology => &mut progress.topology,
        ConstructionPreparationDomain::Placement => &mut progress.placement,
    }
}
