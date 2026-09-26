use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Final presentation prefab selected from the six authored cardinal,
/// diagonal, and curve binders for this fence entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FenceSegmentPrefab {
    pub(crate) asset: AssetId,
    /// Native selected-child Y reflection, applied in source-local coordinates.
    pub(crate) mirror_source_y: bool,
}

/// Endpoint indexes used to fit a fence to the terrain.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FenceTerrainEndpointPresentationCells {
    pub(super) first: IVec3,
    pub(super) second: IVec3,
}

/// Two hierarchy nodes whose transforms reproduce the endpoint skew from
/// `BFSkewComponent`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FenceTerrainEndpointSkewPresentation {
    pub(super) first_transform: Entity,
    pub(super) second_transform: Entity,
}

/// Presentation root to replace when the topology changes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TopologyPresentation(pub(super) Option<Entity>);
