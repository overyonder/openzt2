use bevy::prelude::*;

use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;

use openzt2_zoo_state::topology::topology_edit_types::CommitTopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;

pub(super) fn write_topology_edit_preparation_rejection(
    transaction: Entity,
    reason: PlacementFailure,
    writer: &mut MessageWriter<TopologyEditPreparationRejected>,
) {
    writer.write(TopologyEditPreparationRejected {
        transaction,
        reason,
    });
}

pub(super) fn write_topology_edit_acknowledgement(
    request: &CommitTopologyEdit,
    accepted: bool,
    writer: &mut MessageWriter<TopologyEditAcknowledged>,
) {
    writer.write(TopologyEditAcknowledged {
        transaction: request.transaction,
        application: request.application,
        accepted,
    });
}
