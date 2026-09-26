use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::transportation_and_tours::TransportationTrackKind, AssetId,
};

use crate::plugins::construction::construction_transaction_types::EditApplication;

/// Assigns one already-placed station to the circuit that owns its live
/// topology.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AttachTransportStationRequest {
    pub(super) station: Entity,
    pub(super) circuit: Entity,
}

/// Detaches an unused station from its circuit. Connected endpoints must be
/// disconnected first so an edit cannot strand a live edge.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DetachTransportStationRequest {
    pub(super) station: Entity,
}

/// Attaches one already-placed track entity to two canonical topology
/// endpoints. Endpoints are either authored stations or track junctions.
///
/// The transport construction transaction owns the track entity lifetime. This
/// intent owns only the canonical topology mutation that makes it a circuit
/// edge.
#[derive(Message, Debug, Clone, PartialEq)]
pub(super) struct ConnectTransportTrackRequest {
    pub(super) transaction: Entity,
    pub(super) application: EditApplication,
    pub(super) track: Entity,
    pub(super) circuit: Entity,
    pub(super) from: Entity,
    pub(super) from_endpoint_index: u8,
    pub(super) to: Entity,
    pub(super) to_endpoint_index: u8,
    pub(super) path_points: Box<[Vec3]>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransportTrackConnectionApplied {
    pub(super) transaction: Entity,
    pub(super) application: EditApplication,
    pub(super) track: Entity,
}

/// Removes the topology relation from one track without retaining a detached
/// edge in a parallel construction graph.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DisconnectTransportTrackRequest {
    pub(super) transaction: Entity,
    pub(super) application: EditApplication,
    pub(super) track: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransportTrackDisconnectionApplied {
    pub(super) transaction: Entity,
    pub(super) application: EditApplication,
    pub(super) track: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransportConnectionFailure {
    MissingCircuit,
    MissingStation,
    MissingTrack,
    AlreadyMember,
    AlreadyConnected,
    MissingEndpoint,
    ForeignEndpoint,
    IncompatibleDefinition,
    InvalidGeometry,
    OccupiedEndpoint,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransportTrackConnectionRejected {
    pub(super) transaction: Entity,
    pub(super) application: EditApplication,
    pub(super) track: Entity,
    pub(super) reason: TransportConnectionFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransportStationAssignmentRejected {
    pub(super) station: Entity,
    pub(super) reason: TransportConnectionFailure,
}

/// Installed by construction when deletion of a circuit member requires the
/// authored path confirmation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportPathDeletionPending {
    pub(crate) target: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportStation {
    pub(crate) definition: AssetId,
    pub(crate) capacity: u16,
    pub(crate) queued: u16,
    pub(crate) occupied: u16,
}

/// A canonical junction between committed track segments.
///
/// Its ordinary `GlobalTransform` owns the world position. The marker carries
/// no copied path or transport-definition data.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportTrackJunction;

#[derive(Component, Debug, Clone, PartialEq)]
pub(crate) struct TrackSegment {
    pub(crate) definition: AssetId,
    pub(crate) from: Entity,
    pub(crate) from_endpoint_index: u8,
    pub(crate) from_position: Vec3,
    pub(crate) to: Entity,
    pub(crate) to_endpoint_index: u8,
    pub(crate) to_position: Vec3,
    pub(crate) path_points: Box<[Vec3]>,
    pub(crate) travel_path_points: Box<[Vec3]>,
    pub(crate) length: f32,
}

/// Construction constraints and movement policy attached to the placed track
/// entity. Endpoints remain ordinary entity relations on `TrackSegment`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrackProfile {
    pub(crate) definition: AssetId,
    pub(crate) kind: TransportationTrackKind,
    pub(crate) maximum_grade_permille: u16,
}

/// Authored lower and upper navigation endpoints for one sky tower.
///
/// The endpoints are ordinary entities so navigation and construction retain
/// ownership of their transforms and topology. The tower stores only the two
/// directed relationships needed by its up/down behavior.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkyTowerEndpoints {
    pub(crate) lower: Entity,
    pub(crate) upper: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkyTowerTransitionRequest {
    pub(crate) rider: Entity,
    pub(crate) tower: Entity,
    pub(crate) travel_to_upper_endpoint: bool,
}
