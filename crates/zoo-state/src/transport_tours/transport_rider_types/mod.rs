use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitingForTransport {
    pub station: Entity,
    pub circuit: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportRider {
    pub vehicle: Entity,
    pub boarded_station: Entity,
    pub seat_index: u16,
}

/// Current seat selected by the local player while riding. This is a direct
/// rider fact; scenario rules may capture their own activation baseline.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerRidingSeatIndex(pub u16);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardTransportRequest {
    pub guest: Entity,
    pub station: Entity,
}
