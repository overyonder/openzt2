use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_transaction_types::EditApplication;
use crate::economy::money_types::Money;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use crate::aquatic::aquatic_simulation_types::TankWallTopHeight;
use crate::topology::gate_operation_types::Gate;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyChanged {
    pub transaction: Option<Entity>,
    pub bounds: IRect,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathConnectivityChanged(pub IVec3);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceConnectivityChanged(pub super::topology_graph_types::EdgeKey);

#[derive(Component, Debug, Clone, PartialEq)]
pub struct TopologyEdit {
    pub created: Box<[TopologySnapshot]>,
    pub removed: Box<[TopologySnapshot]>,
}

#[derive(Component, Debug)]
pub struct AppliedFenceTopology(pub Box<[Entity]>);

#[derive(Debug, Clone, PartialEq)]
pub enum TopologySnapshot {
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
        wall_top: Option<TankWallTopHeight>,
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
        endpoint_pose: Option<Transform>,
    },
}

impl TopologySnapshot {
    pub fn id(&self) -> PersistentId {
        match *self {
            Self::Node { id, .. }
            | Self::Fence { id, .. }
            | Self::Path { id, .. }
            | Self::Portal { id, .. } => id,
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitTopologyEdit {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyEditAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}
