use bevy::prelude::*;

/// Stable identity within one terrain asset. Terrain chunks are not zoo
/// objects and therefore do not consume the persistent entity-id namespace.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainChunkId(pub u32);
