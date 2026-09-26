//! Authored tank and aquatic-species habitat definitions.

use crate::AssetId;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TankDefinition {
    pub id: AssetId,
    pub wall: AssetId,
    pub floor_material: AssetId,
    pub water_material: AssetId,
    pub min_depth_cm: u16,
    pub max_depth_cm: u16,
    pub capacity_litres_per_cell: u32,
    pub filtration_per_day: u32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct AquaticRequirement {
    pub species: AssetId,
    pub min_depth_cm: u16,
    pub initial_space_m3: u32,
    pub additional_space_m3: u32,
    pub salinity_permille: [u16; 2],
    pub temperature_c: [i16; 2],
    pub water_quality_min: u16,
    pub land_fraction_permille: [u16; 2],
}

// Authored offsets applied once when a tank surface is hydrated.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TankSurfacePolicy {
    pub water_height_offset_cm: i32,
    pub floor_height_offset_cm: i32,
}
// Global depth requirements shared by tank construction and show eligibility.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TankDepthPolicy {
    pub minimum_water_depth_cm: u32,
    pub minimum_show_tank_depth_cm: u32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TankEditPolicy {
    pub floor_mm_per_second: u32,
    pub wall_mm_per_second: u32,
    pub water_mm_per_second: u32,
    pub fast_multiplier_q16: u32,
    pub slow_multiplier_q16: u32,
    pub fill_speed_fraction_permille: [u16; 2],
    pub drain_speed_fraction_permille: [u16; 2],
}
