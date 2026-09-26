use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::transportation_and_tours::TransportationTrackKind, AssetId,
};

use crate::plugins::{
    construction::{
        construction_interaction_types::PlacementFailure,
        construction_transaction_types::EditApplication,
    },
    economy::money_types::Money,
    world_spawn::persistent_id_types::PersistentId,
};

#[derive(Component, Debug, Clone, PartialEq)]
pub(super) struct TransportTrackConstructionPreview {
    pub(super) circuit: Entity,
    pub(super) from_endpoint: Entity,
    pub(super) from_endpoint_index: u8,
    pub(super) from_position: Vec3,
    pub(super) to_station: Option<Entity>,
    pub(super) to_endpoint_index: u8,
    pub(super) to_position: Vec3,
    pub(super) path_points: Vec<Vec3>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportTrackConstructionPiece {
    pub(crate) route_segment: Entity,
    pub(crate) piece_index: u16,
}

#[derive(Component, Debug, Clone)]
pub(crate) struct PreparedTransportTrackConstructionEdit {
    pub(crate) piece_entities: Box<[PersistentId]>,
    pub(crate) destination_junction_entity: Option<PersistentId>,
    pub(crate) definition: AssetId,
    pub(crate) kind: TransportationTrackKind,
    pub(crate) maximum_grade_permille: u16,
    pub(crate) circuit: Entity,
    pub(crate) from_endpoint: Entity,
    pub(crate) from_endpoint_index: u8,
    pub(crate) to_endpoint: Option<Entity>,
    pub(crate) to_endpoint_index: u8,
    pub(crate) path_points: Box<[Vec3]>,
    pub(crate) applied_entities: Vec<Entity>,
    pub(crate) preview: Option<Entity>,
    pub(crate) application_in_flight: Option<EditApplication>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportTrackConstructionEditPrepared {
    pub(crate) transaction: Entity,
    pub(crate) cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportTrackConstructionEditPreparationRejected {
    pub(crate) transaction: Entity,
    pub(crate) reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApplyPreparedTransportTrackConstructionEditRequest {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportTrackConstructionEditApplicationAcknowledged {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
    pub(crate) accepted: bool,
}

#[derive(Component)]
pub(super) struct TransportTrackConstructionPreviewAuthoredPrefab {
    pub(super) owner: Entity,
}
