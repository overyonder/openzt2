use bevy::prelude::*;

/// Bevy render layer reserved for terrain surfaces and their authored light
/// rig. Ordinary zoo objects remain on layer zero.
pub(crate) const TERRAIN_RENDER_LAYER: usize = 2;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct TerrainRenderChunk;

/// Ordinary Bevy image owned by the chunk material and recomposed after
/// authored surface edits.
#[derive(Component, Debug, Clone)]
pub(super) struct TerrainSurfaceImage(pub Handle<Image>);

#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct TerrainChunkRejected;

/// Last edit revision applied to the mesh and blend image.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TerrainRenderRevision(pub u64);
