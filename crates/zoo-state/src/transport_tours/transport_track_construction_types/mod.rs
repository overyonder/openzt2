use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_data::AssetId;

use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_transaction_types::EditApplication;
use crate::economy::money_types::Money;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

#[derive(Component, Debug, Clone, PartialEq)]
pub struct TransportTrackConstructionPreview {
    pub circuit: Entity,
    pub from_endpoint: Entity,
    pub from_endpoint_index: u8,
    pub from_position: Vec3,
    pub to_station: Option<Entity>,
    pub to_endpoint_index: u8,
    pub to_position: Vec3,
    pub path_points: Vec<Vec3>,
    pub tower_height_m: Option<f32>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = TrackPieces, allow_self_referential)]
pub struct TransportTrackConstructionPiece {
    #[relationship]
    pub route_segment: Entity,
    pub piece_index: u16,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = TransportTrackConstructionPiece)]
pub struct TrackPieces(Vec<Entity>);

#[derive(Component, Debug, Clone)]
pub struct PreparedTransportTrackConstructionEdit {
    pub removes_track: bool,
    pub piece_entities: Box<[PersistentId]>,
    pub destination_junction_entity: Option<PersistentId>,
    /// A junction the edit spawns to start from, as a road left after a gap
    /// starts; `from_endpoint` is then ignored.
    pub source_junction_entity: Option<PersistentId>,
    pub definition: AssetId,
    pub kind: TransportationTrackKind,
    pub maximum_grade_permille: u16,
    pub circuit: Entity,
    pub from_endpoint: Entity,
    pub from_endpoint_index: u8,
    pub to_endpoint: Option<Entity>,
    pub to_endpoint_index: u8,
    pub path_points: Box<[Vec3]>,
    pub tower_height_m: Option<f32>,
    pub applied_entities: Vec<Entity>,
    pub preview: Option<Entity>,
    pub application_in_flight: Option<EditApplication>,
}

/// A transaction made of several route edits, applied in order and undone
/// in reverse.
///
/// Each edit sits on its own child entity; the transaction's own edit only
/// marks it as a transport edit. Removing one piece from the middle of a road
/// removes its route and lays the road left on each side.
#[derive(Component, Debug, Clone, Default)]
pub struct TransportTrackConstructionEditSteps {
    pub application: Option<EditApplication>,
    pub last_application: Option<EditApplication>,
    pub rejected_application: Option<EditApplication>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackConstructionEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackConstructionEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyPreparedTransportTrackConstructionEditRequest {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackConstructionEditApplicationAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}

#[derive(Component)]
pub struct TransportTrackConstructionPreviewAuthoredPrefab;
