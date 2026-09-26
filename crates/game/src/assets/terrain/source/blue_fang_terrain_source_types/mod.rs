//! Parsed Blue Fang terrain source records shared by parsing and lowering.

use openzt2_game_data::terrain::{TerrainSlopeTile, TerrainWaterRegion};

#[derive(Clone, Copy)]
pub(super) struct BlueFangTerrainSourceHeader {
    pub(super) version: u32,
    pub(super) extent_x: f32,
    pub(super) extent_y: f32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) sector_columns: u32,
    pub(super) sector_rows: u32,
}

pub(super) struct ParsedBlueFangTerrainSourceData {
    pub(super) header: BlueFangTerrainSourceHeader,
    pub(super) samples: Vec<ParsedBlueFangTerrainSourceSample>,
    pub(super) slope_tiles: Vec<TerrainSlopeTile>,
    pub(super) biome_names: Vec<String>,
    pub(super) water_regions: Vec<TerrainWaterRegion>,
}

pub(super) struct ParsedBlueFangTerrainSourceSample {
    pub(super) height: f32,
    pub(super) relative_height_offset: Option<f32>,
    pub(super) biome: u32,
    pub(super) ground_cover: u8,
    pub(super) water_type: u32,
    pub(super) water_surface_flag: bool,
    pub(super) excludes_elevated_path: bool,
    pub(super) constraint_flag: Option<u8>,
    pub(super) minimum_height: Option<f32>,
    pub(super) maximum_height: Option<f32>,
    pub(super) linked_height_shape: Option<u8>,
}

pub(in crate::assets) struct ParsedBlueFangTerrain(pub(super) ParsedBlueFangTerrainSourceData);

impl ParsedBlueFangTerrain {
    pub(in crate::assets) fn biome_names(&self) -> &[String] {
        &self.0.biome_names
    }
}
