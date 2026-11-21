use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportStation {
    pub definition: AssetId,
    pub capacity: u16,
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
    pub from_endpoint_index: u8,
    pub from_position: Vec3,
    pub to_endpoint_index: u8,
    pub to_position: Vec3,
    pub path_points: Box<[Vec3]>,
    pub travel_path_points: Box<[Vec3]>,
    pub length: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = OutgoingTracks)]
pub struct TrackStart(pub Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = IncomingTracks)]
pub struct TrackEnd(pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = TrackStart)]
pub struct OutgoingTracks(Vec<Entity>);

#[derive(Component, Debug)]
#[relationship_target(relationship = TrackEnd)]
pub struct IncomingTracks(Vec<Entity>);

#[derive(QueryData)]
pub struct TrackRoute {
    pub geometry: &'static TrackSegment,
    pub start: &'static TrackStart,
    pub end: &'static TrackEnd,
}

impl TrackRouteItem<'_, '_> {
    pub fn from(&self) -> Entity {
        self.start.0
    }
    pub fn to(&self) -> Entity {
        self.end.0
    }
}

impl std::ops::Deref for TrackRouteItem<'_, '_> {
    type Target = TrackSegment;

    fn deref(&self) -> &Self::Target {
        self.geometry
    }
}

#[derive(SystemParam)]
pub struct TrackIncidence<'w, 's> {
    outgoing: Query<'w, 's, &'static OutgoingTracks>,
    incoming: Query<'w, 's, &'static IncomingTracks>,
}

impl TrackIncidence<'_, '_> {
    pub fn outgoing(&self, endpoint: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.outgoing
            .get(endpoint)
            .ok()
            .into_iter()
            .flat_map(|routes| routes.iter())
    }

    pub fn incoming(&self, endpoint: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.incoming
            .get(endpoint)
            .ok()
            .into_iter()
            .flat_map(|routes| routes.iter())
    }

    pub fn incident(&self, endpoint: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.outgoing(endpoint).chain(self.incoming(endpoint))
    }
}

/// Construction constraints and movement policy attached to the placed track
/// entity. Endpoints are linked by `TrackStart` and `TrackEnd`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackProfile {
    pub definition: AssetId,
    pub kind: TransportationTrackKind,
    pub maximum_grade_permille: u16,
}

/// The committed height of a sky tower above its base.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SkyTowerHeight(pub f32);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyTowerEndpoint {
    Lower,
    Upper,
}

#[derive(SystemParam)]
pub struct SkyTowerEndpoints<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    mounts: Query<'w, 's, (&'static SkyTowerEndpoint, &'static GlobalTransform)>,
}

impl SkyTowerEndpoints<'_, '_> {
    pub fn position(&self, tower: Entity, endpoint: SkyTowerEndpoint) -> Option<Vec3> {
        self.children.get(tower).ok()?.iter().find_map(|entity| {
            let (mount, transform) = self.mounts.get(entity).ok()?;
            (*mount == endpoint).then(|| transform.translation())
        })
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyTowerTransitionRequest {
    pub rider: Entity,
    pub tower: Entity,
    pub travel_to_upper_endpoint: bool,
}

/// Presentation restoration for a saved station or vehicle with no live prefab.
#[derive(Component)]
pub struct RestoredTransportPrefab(pub Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>);
