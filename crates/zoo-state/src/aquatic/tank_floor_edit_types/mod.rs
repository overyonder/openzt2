use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_tool_and_placement_policy_types::TankEditKind;
use crate::construction::construction_transaction_types::EditApplication;
use crate::economy::money_types::Money;

use super::aquatic_simulation_types::TankGeometry;

/// Before/after images belong to the construction transaction, never a second live tank.
#[derive(Component, Debug, Clone, Copy)]
pub struct TankFloorEdit {
    pub tank: Entity,
    pub definition: AssetId,
    pub before: TankGeometry,
    pub after: TankGeometry,
}

#[derive(Component)]
pub struct TankFloorGesture {
    pub kind: TankEditKind,
    pub pending: bool,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct TankFloorEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct TankFloorEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct CommitTankFloorEdit {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct TankFloorEditAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}
