use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = StationQueue)]
pub struct WaitingForTransport {
    #[relationship]
    pub station: Entity,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = WaitingForTransport)]
pub struct StationQueue(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = VehicleRiders)]
pub struct TransportRider {
    #[relationship]
    pub vehicle: Entity,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = TransportRider)]
pub struct VehicleRiders(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = StationBoardedRiders)]
pub struct BoardedAtStation(pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = BoardedAtStation)]
pub struct StationBoardedRiders(Vec<Entity>);

/// Current seat selected by the local player while riding. This is a direct
/// rider fact; scenario rules may capture their own activation baseline.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerRidingSeatIndex(pub u16);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardTransportRequest {
    pub guest: Entity,
    pub station: Entity,
}

#[derive(Component)]
pub struct TransportSeat {
    pub index: u16,
    pub back_facing: bool,
}

#[derive(Component)]
pub struct AuthoredTransportSeatPose;

#[derive(Component)]
#[relationship(relationship_target = RidersAtSeat)]
pub struct TransportSeatRider(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TransportSeatRider)]
pub struct RidersAtSeat(Vec<Entity>);

#[derive(Component)]
pub struct TransportStationExit;
