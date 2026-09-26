use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::economy::money_types::Money;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GenerateTransportVehicleForCircuitRequest {
    pub(crate) circuit: Entity,
    pub(crate) definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingTransportVehiclePurchase {
    pub(super) circuit: Entity,
    pub(super) definition: AssetId,
    pub(super) cost: Money,
    pub(super) paid: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransportVehicle {
    pub(crate) definition: AssetId,
    pub(crate) seats: u16,
    pub(crate) occupied: u16,
    pub(crate) maximum_speed_metres_per_second: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoutePosition {
    pub(crate) segment: Entity,
    pub(crate) distance: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StationDestination(pub(super) Entity);
