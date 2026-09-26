//! Authored terrain-editing brush operations, falloff, radius, and strength definitions.

use crate::AssetId;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum TerrainEditingBrushFalloff {
    Constant,
    Linear,
    Smooth,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum TerrainEditingBrushOperation {
    Raise,
    Lower,
    Flatten,
    Smooth,
    PaintBiome,
    AddWater,
    RemoveWater,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TerrainEditingBrushDefinition {
    pub id: AssetId,
    pub radius_cm: [u16; 2],
    pub strength_permille: u16,
    pub falloff: TerrainEditingBrushFalloff,
    pub operation: TerrainEditingBrushOperation,
}
