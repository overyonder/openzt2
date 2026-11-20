use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::staff_assignment_types::{StaffAssignedToArea, StaffAssignedToTarget};

pub(super) fn release_staff_restrictions_when_target_leaves_world(
    removed: On<Remove<WorldMember>>,
    areas: Query<(), With<StaffAssignedToArea>>,
    targets: Query<(), With<StaffAssignedToTarget>>,
    mut commands: Commands,
) {
    if areas.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<StaffAssignedToArea>();
    }
    if targets.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<StaffAssignedToTarget>();
    }
}
