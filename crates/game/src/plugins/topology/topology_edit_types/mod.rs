use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::{
    construction::{
        construction_interaction_types::PlacementFailure,
        construction_transaction_types::EditApplication,
    },
    economy::money_types::Money,
    world_spawn::persistent_id_types::PersistentId,
};

use super::gate_operation_types::Gate;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopologyChanged {
    pub(crate) transaction: Option<Entity>,
    pub(crate) bounds: IRect,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopologyEdit {
    pub(crate) created: Box<[TopologySnapshot]>,
    pub(crate) removed: Box<[TopologySnapshot]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TopologySnapshot {
    Node {
        id: PersistentId,
        cell: IVec3,
    },
    Fence {
        id: PersistentId,
        definition: AssetId,
        a: PersistentId,
        b: PersistentId,
        gate: Option<Gate>,
    },
    Path {
        id: PersistentId,
        definition: AssetId,
        cell: IVec3,
    },
    Portal {
        id: PersistentId,
        destination: PersistentId,
        bidirectional: bool,
    },
}

impl TopologySnapshot {
    pub(crate) fn id(&self) -> PersistentId {
        match *self {
            Self::Node { id, .. }
            | Self::Fence { id, .. }
            | Self::Path { id, .. }
            | Self::Portal { id, .. } => id,
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopologyEditPrepared {
    pub(crate) transaction: Entity,
    pub(crate) cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopologyEditPreparationRejected {
    pub(crate) transaction: Entity,
    pub(crate) reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitTopologyEdit {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopologyEditAcknowledged {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
    pub(crate) accepted: bool,
}
