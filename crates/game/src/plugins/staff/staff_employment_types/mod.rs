use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Staff;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffRole(pub(crate) AssetId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Employment {
    pub(crate) wage: Money,
    pub(crate) hired_tick: u64,
    pub(crate) hired_month_ordinal: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AvailableForWork;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PendingStaffHire {
    pub(crate) role: AssetId,
    pub(crate) position: Vec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingWagePayment {
    pub(crate) staff: Entity,
    pub(crate) due_tick: u64,
}
