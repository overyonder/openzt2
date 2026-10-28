use bevy::prelude::*;

use openzt2_zoo_state::aquatic::tank_geometry_edit_types::CommitTankGeometryEdit;
use openzt2_zoo_state::aquatic::tank_geometry_edit_types::TankGeometryEdit;
use openzt2_zoo_state::placement::placement_transaction_types::ApplyPreparedObjectPlacementEditRequest;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::shows::show_platform_upgrade_types::CommitShowPlatformUpgradeEdit;
use openzt2_zoo_state::shows::show_platform_upgrade_types::ShowPlatformUpgradeEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::CommitTerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEdit;
use openzt2_zoo_state::topology::topology_edit_types::CommitTopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::ApplyPreparedTransportTrackConstructionEditRequest;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::PreparedTransportTrackConstructionEdit;

use openzt2_zoo_state::construction::construction_transaction_types::DomainAckStatus;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::RollbackContext;
use openzt2_zoo_state::construction::construction_transaction_types::RollbackDispatched;

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_construction_failure_rollback_to_changed_domains_once(
    mut commands: Commands,
    construction_transactions: Query<
        (
            Entity,
            Option<&TankGeometryEdit>,
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
    mut tank_requests: MessageWriter<CommitTankGeometryEdit>,
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
        tank_edit,
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
        if tank_edit.is_some() && domain_was_changed(progress.aquatic) {
            tank_requests.write(CommitTankGeometryEdit {
                transaction: transaction_entity,
                application: EditApplication::FailureRollback,
            });
        }
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
