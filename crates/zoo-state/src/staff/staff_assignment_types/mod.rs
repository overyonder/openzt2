use bevy::prelude::*;
use openzt2_game_data::ui_document::action::staff_management::UiWorkerDuty;

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

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleansFilters;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleansRecycling;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptiesTrash;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepsTrash;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetStaffWorkerDutyAssignment {
    pub worker: Entity,
    pub duty: UiWorkerDuty,
    pub assigned: bool,
}

/// Presentation and interaction marker on the UI document owning ordinary
/// staff-assignment tools.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaffAssignmentMode;
