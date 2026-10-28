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
use openzt2_zoo_state::construction::construction_transaction_types::EditApplicationAuthorized;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_authorized_construction_application_to_participating_domains(
    mut authorized_applications: MessageReader<EditApplicationAuthorized>,
    mut construction_transactions: Query<(
        Option<&TankGeometryEdit>,
        Option<&TerrainEdit>,
        Option<&TopologyEdit>,
        Option<&PreparedObjectPlacementEdit>,
        Option<&ShowPlatformUpgradeEdit>,
        Option<&PreparedTransportTrackConstructionEdit>,
        &mut EditCommitProgress,
    )>,
    mut tank_requests: MessageWriter<CommitTankGeometryEdit>,
    mut terrain_application_requests: MessageWriter<CommitTerrainEdit>,
    mut topology_application_requests: MessageWriter<CommitTopologyEdit>,
    mut object_placement_application_requests: MessageWriter<
        ApplyPreparedObjectPlacementEditRequest,
    >,
    mut show_platform_upgrade_application_requests: MessageWriter<CommitShowPlatformUpgradeEdit>,
    mut transport_track_application_requests: MessageWriter<
        ApplyPreparedTransportTrackConstructionEditRequest,
    >,
) {
    for authorized_application in authorized_applications.read() {
        let Ok((
            tank_edit,
            terrain_edit,
            topology_edit,
            placement_edit,
            show_edit,
            transport_track_edit,
            progress,
        )) = construction_transactions.get_mut(authorized_application.transaction)
        else {
            continue;
        };
        if progress.phase != EditCommitPhase::Applying
            || progress.application != authorized_application.application
        {
            continue;
        }
        if tank_edit.is_some() && progress.aquatic == DomainAckStatus::Pending {
            tank_requests.write(CommitTankGeometryEdit {
                transaction: authorized_application.transaction,
                application: authorized_application.application,
            });
        }
        if terrain_edit.is_some() && progress.terrain == DomainAckStatus::Pending {
            terrain_application_requests.write(CommitTerrainEdit {
                transaction: authorized_application.transaction,
                application: authorized_application.application,
            });
        }
        if topology_edit.is_some() && progress.topology == DomainAckStatus::Pending {
            topology_application_requests.write(CommitTopologyEdit {
                transaction: authorized_application.transaction,
                application: authorized_application.application,
            });
        } else if transport_track_edit.is_some() && progress.topology == DomainAckStatus::Pending {
            transport_track_application_requests.write(
                ApplyPreparedTransportTrackConstructionEditRequest {
                    transaction: authorized_application.transaction,
                    application: authorized_application.application,
                },
            );
        }
        if placement_edit.is_some() && progress.placement == DomainAckStatus::Pending {
            object_placement_application_requests.write(ApplyPreparedObjectPlacementEditRequest {
                transaction: authorized_application.transaction,
                application: authorized_application.application,
            });
        }
        if show_edit.is_some() && progress.shows == DomainAckStatus::Pending {
            show_platform_upgrade_application_requests.write(CommitShowPlatformUpgradeEdit {
                transaction: authorized_application.transaction,
                application: authorized_application.application,
            });
        }
    }
}
