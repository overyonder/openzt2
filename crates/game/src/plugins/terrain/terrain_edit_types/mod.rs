use bevy::prelude::*;

use crate::plugins::{
    construction::{
        construction_interaction_types::PlacementFailure,
        construction_transaction_types::EditApplication,
    },
    economy::money_types::Money,
};

use super::terrain_edit_data_types::TerrainEditDelta;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainChanged {
    pub(crate) transaction: Entity,
    pub(crate) chunk: Entity,
    pub(crate) min: UVec2,
    pub(crate) max: UVec2,
}

#[derive(Component, Debug)]
pub(crate) struct TerrainEdit {
    pub(crate) deltas: Vec<TerrainEditDelta>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainEditPrepared {
    pub(crate) transaction: Entity,
    pub(crate) cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainEditPreparationRejected {
    pub(crate) transaction: Entity,
    pub(crate) reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitTerrainEdit {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainEditAcknowledged {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
    pub(crate) accepted: bool,
}

/// Water changed inside this inclusive sample rectangle. Actor systems use
/// the bounds to update occupants of the affected cells.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainWaterChanged {
    pub(crate) chunk: Entity,
    pub(crate) min: UVec2,
    pub(crate) max: UVec2,
}
