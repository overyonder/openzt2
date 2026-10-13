use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Final presentation prefab selected from the six authored cardinal,
/// diagonal, and curve binders for this fence entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceSegmentPrefab {
    pub(crate) asset: AssetId,
    /// Native selected-child Y reflection, applied in source-local coordinates.
    pub(crate) mirror_source_y: bool,
}

/// Endpoint indexes used to fit a fence to the terrain.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceTerrainEndpointPresentationCells {
    pub(super) first: IVec3,
    pub(super) second: IVec3,
}

impl FenceTerrainEndpointPresentationCells {
    pub(crate) const fn endpoint_cells(&self) -> [IVec3; 2] {
        [self.first, self.second]
    }
}

/// Two hierarchy nodes whose transforms reproduce the endpoint skew from
/// `BFSkewComponent`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceTerrainEndpointSkewPresentation {
    pub(super) first_transform: Entity,
    pub(super) second_transform: Entity,
}

impl FenceTerrainEndpointSkewPresentation {
    pub const fn transform_entities(&self) -> [Entity; 2] {
        [self.first_transform, self.second_transform]
    }
}

/// Presentation root to replace when the topology changes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyPresentation(pub(super) Option<Entity>);
