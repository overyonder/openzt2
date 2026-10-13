use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ConstructionCursor {
    pub world: Vec3,
    pub normal: Vec3,
    pub over_terrain: bool,
}

/// The placeable entity under the world pointer while the delete tool is active.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteHoverTarget(pub Entity);

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ConstructionPreview {
    pub definition: AssetId,
    pub transform: Transform,
    pub validity: PlacementValidity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementValidity {
    Pending,
    Valid { cost: Money },
    Invalid(PlacementFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementFailure {
    Unaffordable,
    Locked,
    Occupied,
    OutsideMap,
    TooSteep,
    NoHeadroom,
    InvalidHabitat,
    InvalidTopology,
    AuthoredRule(AssetId),
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitConstruction {
    pub preview: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelConstruction;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteEntity(pub Entity);
