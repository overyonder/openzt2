use bevy::prelude::*;

/// Terrain cells whose navigation data needs updating after an edit.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainNavigationChanged {
    pub chunk: Entity,
    pub min: UVec2,
    pub max: UVec2,
}

/// Restored terrain cells whose graph data must match saved actor routes.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainNavigationRestored {
    pub chunk: Entity,
    pub min: UVec2,
    pub max: UVec2,
}
