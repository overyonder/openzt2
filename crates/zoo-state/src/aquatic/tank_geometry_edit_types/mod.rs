use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_tool_and_placement_policy_types::TankEditKind;
use crate::construction::construction_transaction_types::EditApplication;
use crate::economy::money_types::Money;

use super::aquatic_simulation_types::TankGeometry;

/// Before/after images belong to the construction transaction, never a second
/// live tank. The tank is found through its habitat seed cell, so undo and redo
/// still reach it after its walls were rebuilt.
#[derive(Component, Debug, Clone)]
pub struct TankGeometryEdit {
    pub seed_cell: IVec2,
    pub kind: TankEditKind,
    pub definition: AssetId,
    pub before: TankGeometry,
    pub after: TankGeometry,
    pub wall_heights: Box<[TankWallHeightEdit]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TankWallHeightEdit {
    pub id: openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
    pub before: f32,
    pub after: f32,
    pub minimum: f32,
    pub maximum: f32,
}

#[derive(Component)]
pub struct TankGeometryGesture {
    pub kind: TankEditKind,
    pub pending: bool,
    /// Bounds read from terrain and topology when the gesture began.
    pub bounds: TankHeightBounds,
}

/// World limits on a tank's heights that come from its surroundings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TankHeightBounds {
    pub water_minimum: f32,
    pub minimum_water_depth_m: f32,
    /// Lowest floor from the terrain water-height policy.
    pub floor_minimum: f32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct TankGeometryEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct TankGeometryEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct CommitTankGeometryEdit {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct TankGeometryEditAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}
