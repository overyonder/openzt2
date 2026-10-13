use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceReservation {
    pub facility: Entity,
    pub service: AssetId,
    pub inventory_units: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceProgress {
    pub facility: Entity,
    pub remaining_ticks: u32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceRequest {
    pub customer: Entity,
    pub facility: Entity,
    pub service: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceCompleted {
    pub customer: Entity,
    pub facility: Entity,
    pub service: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePaymentPending {
    pub customer: Entity,
    pub facility: Entity,
    pub service: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePaymentInFlight(pub Entity);
