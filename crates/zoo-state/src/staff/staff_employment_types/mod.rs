use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::{
    StaffJobCapabilityFlags, StaffJobKind,
};
use openzt2_game_data::AssetId;

use crate::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Staff;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffRole(pub AssetId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffJobCapabilities(StaffJobCapabilityFlags);

impl StaffJobCapabilities {
    pub const fn new(capabilities: StaffJobCapabilityFlags) -> Self {
        Self(capabilities)
    }

    pub const fn supports(self, kind: StaffJobKind) -> bool {
        self.0
            .contains_all(StaffJobCapabilityFlags::for_job_kind(kind))
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Employment {
    pub wage: Money,
    pub hired_tick: u64,
    pub hired_month_ordinal: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingWagePayment {
    pub staff: Entity,
    pub due_tick: u64,
}

/// A depth below the water surface that a behavior, such as a keeper
/// cleaning a tank, holds a swimming employee at.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct StaffSubmergedDepth(pub f32);

/// The gate traversal owner controls the employee's height while climbing.
#[derive(Component, Debug)]
pub struct StaffClimbingTankGate;
