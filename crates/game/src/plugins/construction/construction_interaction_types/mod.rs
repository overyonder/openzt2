use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConstructionCursor {
    pub(crate) world: Vec3,
    pub(crate) normal: Vec3,
    pub(crate) over_terrain: bool,
}

/// The placeable entity under the world pointer while the delete tool is active.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeleteHoverTarget(pub(super) Entity);

#[derive(Component, Debug, Clone, PartialEq)]
pub(crate) struct ConstructionPreview {
    pub(crate) definition: AssetId,
    pub(crate) transform: Transform,
    pub(crate) validity: PlacementValidity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacementValidity {
    Pending,
    Valid { cost: Money },
    Invalid(PlacementFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacementFailure {
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
pub(crate) struct CommitConstruction {
    pub(crate) preview: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CancelConstruction;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeleteEntity(pub(crate) Entity);
