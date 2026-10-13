use bevy::prelude::*;
use openzt2_game_data::ui_document::action::staff_management::UiWorkerDuty;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaffAssignment {
    pub area: Option<Entity>,
    pub target: Option<Entity>,
}

/// A world entity that the player has placed in a keeper's assignment set.
/// The relationship remains on the assigned staff entity in [`StaffAssignment`].
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
