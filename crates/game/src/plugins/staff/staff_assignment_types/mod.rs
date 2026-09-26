use bevy::prelude::*;
use openzt2_game_data::ui_document::action::staff_management::UiWorkerDuty;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StaffAssignment {
    pub(crate) area: Option<Entity>,
    pub(crate) target: Option<Entity>,
}

/// A world entity that the player has placed in a keeper's assignment set.
/// The relationship remains on the assigned staff entity in [`StaffAssignment`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KeeperAssignmentTarget;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeeperAssignmentSelection(pub Option<u32>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrainerAssignmentSelection(pub Option<u32>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CleansFilters;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CleansRecycling;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EmptiesTrash;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SweepsTrash;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetStaffWorkerDutyAssignment {
    pub(crate) worker: Entity,
    pub(crate) duty: UiWorkerDuty,
    pub(crate) assigned: bool,
}

/// Presentation and interaction marker on the UI document owning ordinary
/// staff-assignment tools.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StaffAssignmentMode;
