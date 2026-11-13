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
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTrackConstructionPiece {
    pub route_segment: Entity,
    pub piece_index: u16,
}

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
    pub applied_entities: Vec<Entity>,
    pub preview: Option<Entity>,
    pub application_in_flight: Option<EditApplication>,
}

/// A transaction made of several route edits, each on its own child entity,
/// (the transaction's own edit only marks it as a transport edit),
/// applied in order and undone in reverse, as removing one piece from the
/// middle of a road removes its route and lays the two pieces of road left.
#[derive(Component, Debug, Clone, Default)]
pub struct TransportTrackConstructionEditSteps {
    pub steps: Vec<Entity>,
    /// The application in progress and how many steps of it are done.
    pub in_flight: Option<(EditApplication, usize)>,
    /// How many steps the last application completed, for rolling it back.
    pub completed: usize,
    pub last_application: Option<EditApplication>,
    /// The application whose failed step is being rolled back, acknowledged
    /// as failed once the steps before it are reversed.
    pub failed: Option<EditApplication>,
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
pub struct TransportTrackConstructionPreviewAuthoredPrefab {
    pub owner: Entity,
}
