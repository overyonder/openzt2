use bevy::prelude::MessageWriter;

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    save_slot_types::SaveSlotId,
};

pub(super) fn publish_persistence_failure_for_slot_operation(
    persistence_failure_messages: &mut MessageWriter<WorldSnapshotPersistenceFailed>,
    persistence_operation: WorldSnapshotPersistenceOperation,
    save_slot_identifier: SaveSlotId,
    failure_reason: WorldSnapshotPersistenceFailure,
) {
    persistence_failure_messages.write(WorldSnapshotPersistenceFailed {
        persistence_operation,
        save_slot_identifier,
        failure_reason,
    });
}
