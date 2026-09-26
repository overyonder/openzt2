use bevy::prelude::*;

use super::terrain_chunk_identity_type::TerrainChunkId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainEditDelta {
    pub chunk: TerrainChunkId,
    pub min: UVec2,
    pub max: UVec2,
    pub before: Box<[TerrainSample]>,
    pub after: Box<[TerrainSample]>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainSample {
    pub height_cm: i16,
    pub blend: [u8; 16],
    pub ground_cover: u8,
    /// Zero for dry samples; otherwise the one-based loaded water-style row.
    pub water_style: u16,
    /// Water configuration selected by the brush, independent of ground blending.
    pub water_biome_channel: u8,
    pub water_cm: i16,
}
