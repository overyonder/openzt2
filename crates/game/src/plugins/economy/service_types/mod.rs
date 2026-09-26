use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceReservation {
    pub(crate) facility: Entity,
    pub(crate) service: AssetId,
    pub(crate) inventory_units: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceProgress {
    pub(crate) facility: Entity,
    pub(crate) remaining_ticks: u32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceRequest {
    pub(crate) customer: Entity,
    pub(crate) facility: Entity,
    pub(crate) service: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceCompleted {
    pub(crate) customer: Entity,
    pub(crate) facility: Entity,
    pub(crate) service: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ServicePaymentPending {
    pub(super) customer: Entity,
    pub(super) facility: Entity,
    pub(super) service: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ServicePaymentInFlight(pub(super) Entity);
