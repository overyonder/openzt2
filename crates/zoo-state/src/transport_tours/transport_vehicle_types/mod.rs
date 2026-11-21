use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerateTransportVehicleForCircuitRequest {
    pub circuit: Entity,
    pub definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[require(TransportVehicleArrival)]
pub struct TransportVehicle {
    pub definition: AssetId,
    pub seats: u16,
    pub maximum_speed_metres_per_second: f32,
}

#[derive(Component, Debug, Default)]
pub struct TransportVehicleArrival(pub Option<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct RoutePosition {
    pub distance: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = VehiclesOnTrack)]
pub struct VehicleOnTrack(pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = VehicleOnTrack)]
pub struct VehiclesOnTrack(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StationDestination(pub Entity);
