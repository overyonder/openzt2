use bevy::prelude::*;

use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_transaction_types::EditApplication;
use crate::economy::money_types::Money;

use crate::terrain::terrain_edit_data_types::TerrainEditDelta;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainChanged {
    pub transaction: Entity,
    pub chunk: Entity,
    pub min: UVec2,
    pub max: UVec2,
}

#[derive(Component, Debug)]
pub struct TerrainEdit {
    pub deltas: Vec<TerrainEditDelta>,
    pub water_regions:
        Vec<crate::terrain::terrain_water_presentation_types::TerrainWaterRegionEditDelta>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

/// A behavior put down an object whose placement flattens terrain, outside
/// any construction transaction.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct FlattenTerrainUnderBehaviorPlacement {
    pub definition: openzt2_game_data::AssetId,
    pub transform: Transform,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitTerrainEdit {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainEditAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}

/// Water changed inside this inclusive sample rectangle. Actor systems use
/// the bounds to update occupants of the affected cells.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainWaterChanged {
    pub chunk: Entity,
    pub min: UVec2,
    pub max: UVec2,
}
