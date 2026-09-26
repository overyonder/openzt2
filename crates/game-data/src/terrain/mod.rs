//! Authored terrain-grid data retained beside its ordinary glTF mesh.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainGrid {
    pub width: u32,
    pub height: u32,
    pub sector_columns: u32,
    pub sector_rows: u32,
    pub units: TerrainUnits,
    pub presentation: TerrainPresentation,
    pub biomes: Vec<TerrainBiome>,
    pub samples: Vec<TerrainSample>,
    pub water_regions: Vec<TerrainWaterRegion>,
    pub waterfall: Option<TerrainWaterfall>,
    pub slope_tiles: Vec<TerrainSlopeTile>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainPresentation {
    pub detail_texture: String,
    pub detail_texture_repetition: f32,
    pub cliff_rise_per_metre: [f32; 2],
    pub surface_resolution: u16,
    pub surface_margin_cells: u8,
    pub full_alpha_composition: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainBiome {
    pub name: String,
    pub ground_texture: String,
    pub cover_texture: String,
    pub mix_mask: String,
    pub cliff_textures: [String; 5],
    pub ground_brushes: [String; 4],
    pub cover_brushes: [String; 4],
    pub ground_no_blend_brushes: [String; 4],
    pub cover_no_blend_brushes: [String; 4],
    pub draw_order: i16,
    pub shallow_water_depth_mm: Option<i32>,
    pub deep_water_depth_mm: Option<i32>,
    /// Native water-side attachment upper offset; absent source defaults to zero.
    #[serde(default)]
    pub water_shore_offset_mm: Option<i32>,
    pub water_presentation: Option<TerrainBiomeWaterPresentation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainBiomeWaterPresentation {
    pub vertex_colour_low: [u8; 4],
    pub vertex_colour_medium: [u8; 4],
    pub vertex_colour_high: [u8; 4],
    pub gloss_map: String,
    pub texture_animations: [TerrainWaterTextureAnimation; 2],
    pub surface_material: TerrainWaterSurfaceMaterialPresentation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterSurfaceMaterialPresentation {
    pub reflection: TerrainWaterReflectionPresentation,
    pub refraction: TerrainWaterRefractionPresentation,
    pub geometric_waves: TerrainWaterGeometricWavePresentation,
    pub ripple_waves: TerrainWaterRippleWavePresentation,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterReflectionPresentation {
    pub strength_percent: f32,
    pub ambient_colour_rgb255: [u8; 3],
    pub tint_colour_rgb255: [u8; 3],
    pub map_size_metres: f32,
    pub bumpiness_percent: f32,
    pub falloff_metres: f32,
    pub fresnel_percent: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterRefractionPresentation {
    pub ambient_colour_rgb255: [u8; 3],
    pub tint_colour_rgb255: [u8; 3],
    pub map_size_metres: f32,
    pub bumpiness_percent: f32,
    pub falloff_metres: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterGeometricWavePresentation {
    pub minimum_amplitude_percent: f32,
    pub maximum_amplitude_percent: f32,
    pub chop_percent: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterRippleWavePresentation {
    pub lifespan_seconds: f32,
    pub startup_seconds: f32,
    pub minimum_amplitude_percent: f32,
    pub maximum_amplitude_percent: f32,
    pub chop_percent: f32,
    pub speed_metres_per_second: f32,
    pub ramp_minimum_metres: f32,
    pub ramp_maximum_metres: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterTextureAnimation {
    pub texture: String,
    pub grid_dimensions: [f32; 2],
    pub start_minimum_uv: [f32; 2],
    pub start_maximum_uv: [f32; 2],
    pub end_minimum_uv: [f32; 2],
    pub end_maximum_uv: [f32; 2],
    pub scroll_interval: f32,
    pub scroll_texture: bool,
    pub ping_pong: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainUnits {
    pub cell_size_metres: f32,
    pub base_height_metres: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainSample {
    pub height_relative_to_base_metres: f32,
    pub biome_index: u8,
    pub has_ground_cover: bool,
    pub water_depth: TerrainWaterDepth,
    /// Independent authored surface flag used by land/water eligibility.
    pub water_surface_flag: bool,
    /// Exclude elevated-path attachment unless overridden by the terrain manager.
    pub excludes_elevated_path: bool,
    /// Absolute native cell support floor; the map base must not be added.
    pub support_floor_metres: Option<f32>,
    /// Absolute native cell height cap; the map base must not be added.
    pub height_cap_metres: Option<f32>,
    pub height_is_linked: bool,
    /// Independent native child-height link flag (source byte's low bit).
    pub has_child_height_link: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
pub enum TerrainWaterDepth {
    Dry,
    Shallow,
    Deep,
}

// These booleans preserve independent waterfall material and placement flags.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent authored waterfall flags"
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterfall {
    pub decal_mask: String,
    pub decal_detail: String,
    /// Scene asset which owns the authored particle-system dependencies.
    pub particle_scene: String,
    /// Standard audio asset when the source sound component names one.
    pub sound: Option<String>,
    pub decal_width_metres: f32,
    pub decal_height_metres: f32,
    pub height_offset_metres: f32,
    pub detail_v_scroll: f32,
    pub particle_scrunch: f32,
    pub alpha_blend: bool,
    pub double_sided: bool,
    pub rotate_detail: bool,
    pub float_on_water: bool,
    pub water_effect: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainWaterRegion {
    pub height_relative_to_base_metres: f32,
    pub water_style_index: u32,
    pub row_major_cell_indices: Vec<u32>,
    pub depth_ratio: f32,
    /// Use the tank minimum depth when checking surface attachment bounds.
    pub uses_tank_minimum_depth: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerrainSlopeTile {
    pub uses_alternate_diagonal: bool,
}
