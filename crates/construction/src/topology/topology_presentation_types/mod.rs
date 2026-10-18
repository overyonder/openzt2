use bevy::prelude::*;

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
