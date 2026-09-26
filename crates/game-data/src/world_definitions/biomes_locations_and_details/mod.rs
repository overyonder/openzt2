//! Authored biome, world location, and environmental detail-placement definitions.

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeDefinition {
    pub id: AssetId,
    pub name_key: AssetId,
    pub icon: AssetId,
    pub terrain_material: AssetId,
    pub cliff_horizontal: AssetId,
    pub cliff_vertical: AssetId,
    pub cliff_diagonal_ascending: AssetId,
    pub cliff_diagonal_descending: AssetId,
    pub cliff_unaligned: AssetId,
    pub water_material: AssetId,
    /// World-aligned authored byte mask sampled by biome automatic placement.
    pub automatic_placement_mask: AssetId,
    pub automatic_placement_variations: Vec<BiomeAutomaticPlacementVariation>,
    pub foliage: Vec<AssetId>,
    pub detail_policies: Vec<BiomeDetailPolicy>,
    pub overview_colors: BiomeOverviewMapColors,
    pub temperature_c: [i16; 2],
    pub humidity_permille: [u16; 2],
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeOverviewMapColors {
    pub covered_ground: [u8; 4],
    pub uncovered_ground: [u8; 4],
    pub cliff: [u8; 4],
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum BiomeAutomaticPlacementVariationKind {
    FoliageMix,
    FoliageMixTerrain,
    Water,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeAutomaticPlacementVariation {
    pub kind: BiomeAutomaticPlacementVariationKind,
    pub ranges: Vec<BiomeAutomaticPlacementRange>,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeAutomaticPlacementRange {
    pub maximum_mask_value: u8,
    pub choices: Vec<BiomeAutomaticPlacementChoice>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeAutomaticPlacementChoice {
    /// `None` is the source-authored `nothing` outcome.
    pub object: Option<AssetId>,
    pub weight: u32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct WorldLocationDefinition {
    pub id: AssetId,
    pub name_key: AssetId,
    pub icon: AssetId,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum BiomeDetailLevel {
    Lowest,
    Low,
    Medium,
    High,
    Decorative,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum BiomeDetailSurface {
    Ground,
    Cover,
    Shore,
}
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeDetailPolicy {
    pub level: BiomeDetailLevel,
    pub surface: BiomeDetailSurface,
    pub density: u16,
    pub choices: Vec<BiomeDetailChoice>,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeDetailChoice {
    pub prefab: AssetId,
    pub weight: u32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BiomeDetailPlacementPolicy {
    pub region_size_cm: u16,
    pub radius_cm: u16,
    pub solid_distance_cm: u16,
    pub fade_distance_cm: u16,
}
