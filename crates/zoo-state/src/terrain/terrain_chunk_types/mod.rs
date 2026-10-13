use std::collections::HashMap;

use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

#[derive(Component, Debug, Clone)]
pub struct TerrainChunk {
    /// Grid coordinate in the source Z-up terrain record.
    pub source_coord: IVec2,
    /// Grid coordinate in the converted Bevy XZ world.
    pub coord: IVec2,
    pub asset: Handle<TerrainAsset>,
    pub origin: Vec2,
    pub spacing_m: f32,
    pub side: u16,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct EditedTerrainSamples {
    pub heights_cm: Box<[i16]>,
    /// Canonical interleaved R8 terrain blend weights (one byte per authored
    /// biome channel; shipped maps contain up to sixteen channels).
    pub blend_weights: Box<[u8]>,
    pub ground_cover: Box<[u8]>,
    /// Zero for dry samples; otherwise the one-based loaded water-style row.
    pub water_styles: Box<[u16]>,
    pub water_biome_channels: Box<[u8]>,
    pub water_cm: Box<[i16]>,
}

#[derive(Resource, Debug, Default)]
pub struct TerrainIndex {
    pub chunks: HashMap<IVec2, Entity>,
    pub origin: Vec2,
    pub chunk_span_m: f32,
}
