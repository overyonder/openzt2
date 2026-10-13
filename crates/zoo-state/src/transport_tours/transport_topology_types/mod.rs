use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_data::AssetId;

use crate::construction::construction_transaction_types::EditApplication;

/// Assigns one already-placed station to the circuit that owns its live
/// topology.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachTransportStationRequest {
    pub station: Entity,
    pub circuit: Entity,
}

/// Detaches an unused station from its circuit. Connected endpoints must be
/// disconnected first so an edit cannot strand a live edge.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetachTransportStationRequest {
    pub station: Entity,
}

/// Attaches one already-placed track entity to two canonical topology
/// endpoints. Endpoints are either authored stations or track junctions.
///
/// The transport construction transaction owns the track entity lifetime. This
/// intent owns only the canonical topology mutation that makes it a circuit
/// edge.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ConnectTransportTrackRequest {
    pub transaction: Entity,
    pub application: EditApplication,
    pub track: Entity,
    pub circuit: Entity,
    pub from: Entity,
    pub from_endpoint_index: u8,
    pub to: Entity,
    pub to_endpoint_index: u8,
    pub path_points: Box<[Vec3]>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackConnectionApplied {
    pub transaction: Entity,
    pub application: EditApplication,
    pub track: Entity,
}

/// Removes the topology relation from one track without retaining a detached
/// edge in a parallel construction graph.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisconnectTransportTrackRequest {
    pub transaction: Entity,
    pub application: EditApplication,
    pub track: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackDisconnectionApplied {
    pub transaction: Entity,
    pub application: EditApplication,
    pub track: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportConnectionFailure {
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
pub struct TransportTrackConnectionRejected {
    pub transaction: Entity,
    pub application: EditApplication,
    pub track: Entity,
    pub reason: TransportConnectionFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportStationAssignmentRejected {
    pub station: Entity,
    pub reason: TransportConnectionFailure,
}

/// Installed by construction when deletion of a circuit member requires the
/// authored path confirmation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportPathDeletionPending {
    pub target: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportStation {
    pub definition: AssetId,
    pub capacity: u16,
    pub queued: u16,
    pub occupied: u16,
}

/// A canonical junction between committed track segments.
///
/// Its ordinary `GlobalTransform` owns the world position. The marker carries
/// no copied path or transport-definition data.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackJunction;

#[derive(Component, Debug, Clone, PartialEq)]
pub struct TrackSegment {
    pub definition: AssetId,
    pub from: Entity,
    pub from_endpoint_index: u8,
    pub from_position: Vec3,
    pub to: Entity,
    pub to_endpoint_index: u8,
    pub to_position: Vec3,
    pub path_points: Box<[Vec3]>,
    pub travel_path_points: Box<[Vec3]>,
    pub length: f32,
}

/// Construction constraints and movement policy attached to the placed track
/// entity. Endpoints remain ordinary entity relations on `TrackSegment`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackProfile {
    pub definition: AssetId,
    pub kind: TransportationTrackKind,
    pub maximum_grade_permille: u16,
}

/// Authored lower and upper navigation endpoints for one sky tower.
///
/// The endpoints are ordinary entities so navigation and construction retain
/// ownership of their transforms and topology. The tower stores only the two
/// directed relationships needed by its up/down behavior.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyTowerEndpoints {
    pub lower: Entity,
    pub upper: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyTowerTransitionRequest {
    pub rider: Entity,
    pub tower: Entity,
    pub travel_to_upper_endpoint: bool,
}
