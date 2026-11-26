use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = StaffAssignedToArea)]
pub struct StaffAssignmentArea(pub Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = StaffAssignmentArea)]
pub struct StaffAssignedToArea(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = StaffAssignedToTarget)]
pub struct StaffAssignmentTarget(pub Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = StaffAssignmentTarget)]
pub struct StaffAssignedToTarget(Vec<Entity>);

/// A world entity that the player has placed in a keeper's assignment set.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeeperAssignmentTarget;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeeperAssignmentSelection(pub Option<u32>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerAssignmentSelection(pub Option<u32>);

/// Player overrides of the worker's default authored duty assignments.
#[derive(Component, Debug, Default, PartialEq, Eq)]
pub struct WorkerDutyAssignments(std::collections::BTreeSet<AssetId>);

impl WorkerDutyAssignments {
    pub fn is_assigned(&self, task: AssetId) -> bool {
        !self.0.contains(&task)
    }

    pub fn set_assigned(&mut self, task: AssetId, assigned: bool) {
        if assigned {
            self.0.remove(&task);
        } else {
            self.0.insert(task);
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetStaffWorkerDutyAssignment {
    pub worker: Entity,
    pub task: AssetId,
    pub assigned: bool,
}

/// Presentation and interaction marker on the UI document owning ordinary
/// staff-assignment tools.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaffAssignmentMode;
