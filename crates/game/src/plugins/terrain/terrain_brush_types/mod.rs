use bevy::prelude::*;
use openzt2_game_data::{
    terrain::TerrainWaterDepth,
    world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind, AssetId,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TerrainBrushKind {
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
pub(crate) enum TerrainBrushFalloff {
    Constant,
    Linear,
    Smooth,
    Cosine,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TerrainBrushPreview {
    pub(crate) center: Vec2,
    pub(crate) radius_m: f32,
    pub(crate) strength_per_s: f32,
    pub(crate) seconds: f32,
    pub(crate) kind: TerrainBrushKind,
    pub(crate) falloff: TerrainBrushFalloff,
    pub(crate) paint_trees: bool,
    pub(crate) paint_foliage: bool,
    pub(crate) paint_rocks: bool,
    pub(crate) automatic_placement_variation: BiomeAutomaticPlacementVariationKind,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct DefinedTerrainStroke {
    pub(crate) preview: Entity,
    pub(crate) definition: AssetId,
    pub(crate) center: Vec2,
    pub(crate) radius_permille: u16,
    pub(crate) pressure_permille: u16,
    pub(crate) seconds: f32,
    pub(crate) flatten_height_cm: i16,
    pub(crate) paint_biome: Option<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TerrainStroke {
    pub(crate) preview: Entity,
    pub(crate) center: Vec2,
    pub(crate) radius_m: f32,
    pub(crate) strength_per_s: f32,
    pub(crate) seconds: f32,
    pub(crate) kind: TerrainBrushKind,
    pub(crate) falloff: TerrainBrushFalloff,
    pub(crate) paint_trees: bool,
    pub(crate) paint_foliage: bool,
    pub(crate) paint_rocks: bool,
    pub(crate) automatic_placement_variation: BiomeAutomaticPlacementVariationKind,
}
