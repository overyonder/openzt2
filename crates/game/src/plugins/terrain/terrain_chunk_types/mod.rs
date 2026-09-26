use std::collections::HashMap;

use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

#[derive(Component, Debug, Clone)]
pub(crate) struct TerrainChunk {
    /// Grid coordinate in the source Z-up terrain record.
    pub(crate) source_coord: IVec2,
    /// Grid coordinate in the converted Bevy XZ world.
    pub(crate) coord: IVec2,
    pub(crate) asset: Handle<TerrainAsset>,
    pub(crate) origin: Vec2,
    pub(crate) spacing_m: f32,
    pub(crate) side: u16,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditedTerrainSamples {
    pub(crate) heights_cm: Box<[i16]>,
    /// Canonical interleaved R8 terrain blend weights (one byte per authored
    /// biome channel; shipped maps contain up to sixteen channels).
    pub(crate) blend_weights: Box<[u8]>,
    pub(crate) ground_cover: Box<[u8]>,
    /// Zero for dry samples; otherwise the one-based loaded water-style row.
    pub(crate) water_styles: Box<[u16]>,
    pub(crate) water_biome_channels: Box<[u8]>,
    pub(crate) water_cm: Box<[i16]>,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct TerrainIndex {
    pub(crate) chunks: HashMap<IVec2, Entity>,
    pub(crate) origin: Vec2,
    pub(crate) chunk_span_m: f32,
}
