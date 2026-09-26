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
    DomainAckStatus, EditApplicationAuthorized, EditCommitPhase, EditCommitProgress,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_authorized_construction_application_to_participating_domains(
    mut authorized_applications: MessageReader<EditApplicationAuthorized>,
    mut construction_transactions: Query<(
        Option<&TerrainEdit>,
        Option<&TopologyEdit>,
        Option<&PreparedObjectPlacementEdit>,
        Option<&ShowPlatformUpgradeEdit>,
        Option<&PreparedTransportTrackConstructionEdit>,
        &mut EditCommitProgress,
    )>,
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
