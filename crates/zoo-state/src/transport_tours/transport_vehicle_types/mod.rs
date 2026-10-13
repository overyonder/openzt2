use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::economy::money_types::Money;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerateTransportVehicleForCircuitRequest {
    pub circuit: Entity,
    pub definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTransportVehiclePurchase {
    pub circuit: Entity,
    pub definition: AssetId,
    pub cost: Money,
    pub paid: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TransportVehicle {
    pub definition: AssetId,
    pub seats: u16,
    pub occupied: u16,
    pub maximum_speed_metres_per_second: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct RoutePosition {
    pub segment: Entity,
    pub distance: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StationDestination(pub Entity);
