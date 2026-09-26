use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WaitingForTransport {
    pub(crate) station: Entity,
    pub(crate) circuit: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportRider {
    pub(crate) vehicle: Entity,
    pub(crate) boarded_station: Entity,
    pub(crate) seat_index: u16,
}

/// Current seat selected by the local player while riding. This is a direct
/// rider fact; scenario rules may capture their own activation baseline.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerRidingSeatIndex(pub(crate) u16);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoardTransportRequest {
    pub(crate) guest: Entity,
    pub(crate) station: Entity,
}
