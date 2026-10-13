use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::{
    construction::{
        construction_interaction_types::PlacementFailure,
        construction_tool_and_placement_policy_types::TankEditKind,
        construction_transaction_types::EditApplication,
    },
    economy::money_types::Money,
};

use super::aquatic_simulation_types::TankGeometry;

/// Before/after images belong to the construction transaction, never a second live tank.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TankFloorEdit {
    pub(super) tank: Entity,
    pub(super) definition: AssetId,
    pub(super) before: TankGeometry,
    pub(super) after: TankGeometry,
}

#[derive(Component)]
pub(super) struct TankFloorGesture {
    pub(super) kind: TankEditKind,
    pub(super) pending: bool,
}

#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct TankFloorEditPrepared {
    pub(crate) transaction: Entity,
    pub(crate) cost: Money,
}

#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct TankFloorEditPreparationRejected {
    pub(crate) transaction: Entity,
    pub(crate) reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct CommitTankFloorEdit {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct TankFloorEditAcknowledged {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
    pub(crate) accepted: bool,
}
