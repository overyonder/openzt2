use bevy::prelude::*;

use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{staff_assignment_types::StaffAssignment, staff_employment_types::Staff};

/// Removes dead world relationships from the canonical assignment component.
///
/// Assignment targets are restrictions, not ownership handles. Deleting an
/// area or target makes the worker generally available again.
pub(in crate::plugins::staff) fn clear_assignments_to_despawned_world_entities(
    mut staff_assignments: Query<&mut StaffAssignment, With<Staff>>,
    world_entities: Query<(), With<WorldMember>>,
) {
    for mut staff_assignment in &mut staff_assignments {
        if staff_assignment
            .area
            .is_some_and(|area| world_entities.get(area).is_err())
        {
            staff_assignment.area = None;
        }
        if staff_assignment
            .target
            .is_some_and(|target| world_entities.get(target).is_err())
        {
            staff_assignment.target = None;
        }
    }
}
