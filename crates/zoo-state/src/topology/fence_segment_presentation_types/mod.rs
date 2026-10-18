use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Final authored segment and reflection selected for an existing fence edge.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceSegmentPrefab {
    pub asset: AssetId,
    pub mirror_source_y: bool,
    pub shape: FenceSegmentShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceSegmentShape {
    Straight,
    CardinalCurve90,
    DiagonalCurve90,
    CardinalCurve135,
    DiagonalCurve135,
}

/// Ordered endpoints used by the selected segment's presentation and fitting.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceTerrainEndpointPresentationCells {
    pub first: IVec3,
    pub second: IVec3,
}

impl FenceTerrainEndpointPresentationCells {
    pub const fn endpoint_cells(&self) -> [IVec3; 2] {
        [self.first, self.second]
    }
}
