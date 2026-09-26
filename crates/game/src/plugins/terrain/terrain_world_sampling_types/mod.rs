use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainPoint {
    pub height_m: f32,
    pub water_height_m: Option<f32>,
    pub surface: u32,
    pub normal: Vec3,
}

/// Biome and water properties at one terrain sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainBiomePoint {
    pub id: AssetId,
    pub channel: u8,
    pub weight: u8,
}

/// Water policy selected by the authored surface sample at one terrain point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainWaterPoint {
    pub biome_channel: u8,
    /// Authored DAT water type.
    pub style: u16,
    pub id: AssetId,
    pub appearance: AssetId,
    pub flags: u8,
}

/// Combined immutable geometry, biome and water facts sampled at one point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuthoredTerrainPoint {
    pub geometry: TerrainPoint,
    pub biome: Option<TerrainBiomePoint>,
    pub water: Option<TerrainWaterPoint>,
}
