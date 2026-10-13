use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind;
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TerrainBrushKind {
    Raise,
    Lower,
    Smooth,
    Flatten {
        height_cm: i16,
    },
    Paint {
        biome: AssetId,
        /// `Some` selects the authored ground/cover surface; `None` retains
        /// each destination sample's current surface kind.
        ground_cover: Option<bool>,
    },
    PaintWater {
        biome: AssetId,
        depth: TerrainWaterDepth,
    },
    AddWater,
    RemoveWater,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainBrushFalloff {
    Constant,
    Linear,
    Smooth,
    Cosine,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TerrainBrushPreview {
    pub center: Vec2,
    pub radius_m: f32,
    pub strength_per_s: f32,
    pub seconds: f32,
    pub kind: TerrainBrushKind,
    pub falloff: TerrainBrushFalloff,
    pub paint_trees: bool,
    pub paint_foliage: bool,
    pub paint_rocks: bool,
    pub automatic_placement_variation: BiomeAutomaticPlacementVariationKind,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DefinedTerrainStroke {
    pub preview: Entity,
    pub definition: AssetId,
    pub center: Vec2,
    pub radius_permille: u16,
    pub pressure_permille: u16,
    pub seconds: f32,
    pub flatten_height_cm: i16,
    pub paint_biome: Option<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct TerrainStroke {
    pub preview: Entity,
    pub center: Vec2,
    pub radius_m: f32,
    pub strength_per_s: f32,
    pub seconds: f32,
    pub kind: TerrainBrushKind,
    pub falloff: TerrainBrushFalloff,
    pub paint_trees: bool,
    pub paint_foliage: bool,
    pub paint_rocks: bool,
    pub automatic_placement_variation: BiomeAutomaticPlacementVariationKind,
}
