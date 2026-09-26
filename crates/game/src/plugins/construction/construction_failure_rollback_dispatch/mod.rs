use bevy::prelude::*;

use crate::plugins::{
    placement::placement_transaction_types::{
        ApplyPreparedObjectPlacementEditRequest, PreparedObjectPlacementEdit,
    },
    shows::show_platform_upgrade_types::{CommitShowPlatformUpgradeEdit, ShowPlatformUpgradeEdit},
    terrain::terrain_edit_types::{CommitTerrainEdit, TerrainEdit},
    topology::topology_edit_types::{CommitTopologyEdit, TopologyEdit},
    transport_tours::transport_track_construction_types::{
        ApplyPreparedTransportTrackConstructionEditRequest, PreparedTransportTrackConstructionEdit,
    },
};

use super::construction_transaction_types::{
    DomainAckStatus, EditApplication, EditCommitPhase, EditCommitProgress, RollbackContext,
    RollbackDispatched,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_construction_failure_rollback_to_changed_domains_once(
    mut commands: Commands,
    construction_transactions: Query<
        (
            Entity,
            Option<&TerrainEdit>,
            Option<&TopologyEdit>,
            Option<&PreparedObjectPlacementEdit>,
            Option<&ShowPlatformUpgradeEdit>,
            Option<&PreparedTransportTrackConstructionEdit>,
            &EditCommitProgress,
            &RollbackContext,
        ),
        Without<RollbackDispatched>,
    >,
    mut terrain_rollback_requests: MessageWriter<CommitTerrainEdit>,
    mut topology_rollback_requests: MessageWriter<CommitTopologyEdit>,
    mut object_placement_rollback_requests: MessageWriter<ApplyPreparedObjectPlacementEditRequest>,
    mut show_platform_upgrade_rollback_requests: MessageWriter<CommitShowPlatformUpgradeEdit>,
    mut transport_track_rollback_requests: MessageWriter<
        ApplyPreparedTransportTrackConstructionEditRequest,
    >,
) {
    for (
        transaction_entity,
        terrain_edit,
        topology_edit,
        placement_edit,
        show_edit,
        transport_track_edit,
        progress,
        rollback_context,
    ) in &construction_transactions
    {
        if progress.phase != EditCommitPhase::RollingBack
            || progress.application != EditApplication::FailureRollback
        {
            continue;
        }
        let domain_was_changed = |domain_status| {
            if rollback_context.failed_application == EditApplication::Undo {
                domain_status == DomainAckStatus::Reverted
            } else {
                domain_status == DomainAckStatus::Applied
            }
        };
        if terrain_edit.is_some() && domain_was_changed(progress.terrain) {
            terrain_rollback_requests.write(CommitTerrainEdit {
                transaction: transaction_entity,
                application: EditApplication::FailureRollback,
            });
        }
        if topology_edit.is_some() && domain_was_changed(progress.topology) {
            topology_rollback_requests.write(CommitTopologyEdit {
                transaction: transaction_entity,
                application: EditApplication::FailureRollback,
            });
        } else if transport_track_edit.is_some() && domain_was_changed(progress.topology) {
            transport_track_rollback_requests.write(
                ApplyPreparedTransportTrackConstructionEditRequest {
                    transaction: transaction_entity,
                    application: EditApplication::FailureRollback,
                },
            );
        }
        if placement_edit.is_some() && domain_was_changed(progress.placement) {
            object_placement_rollback_requests.write(ApplyPreparedObjectPlacementEditRequest {
                transaction: transaction_entity,
                application: EditApplication::FailureRollback,
            });
        }
        if show_edit.is_some() && domain_was_changed(progress.shows) {
            show_platform_upgrade_rollback_requests.write(CommitShowPlatformUpgradeEdit {
                transaction: transaction_entity,
                application: EditApplication::FailureRollback,
            });
        }
        commands
            .entity(transaction_entity)
            .insert(RollbackDispatched);
    }
}
