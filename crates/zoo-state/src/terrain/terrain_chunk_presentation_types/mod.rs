use bevy::prelude::*;

/// Bevy render layer reserved for terrain surfaces and their authored light
/// rig. Ordinary zoo objects remain on layer zero.
pub const TERRAIN_RENDER_LAYER: usize = 2;

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainRenderChunk;

/// Ordinary Bevy image owned by the chunk material and recomposed after
/// authored surface edits.
#[derive(Component, Debug, Clone)]
pub struct TerrainSurfaceImage(pub Handle<Image>);

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainChunkRejected;

/// Last edit revision applied to the mesh and blend image.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainRenderRevision(pub u64);

/// A texture painted into the terrain surface image.
///
/// The original paints a road (`paintTexture`) into the ground. It covers the square of `size_m`
/// around `centre` (world XZ) with the `uv_min`..`uv_max` part of `texture`,
/// U along +X (east) and V along -Z (north), blended by twice the texture's alpha.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct TerrainPaintedTexture {
    pub centre: Vec2,
    pub size_m: f32,
    pub texture: Handle<Image>,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
}
