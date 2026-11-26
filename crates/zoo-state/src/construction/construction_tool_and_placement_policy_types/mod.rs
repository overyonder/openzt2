use bevy::prelude::*;
use openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind;
use openzt2_game_data::AssetId;

use crate::terrain::terrain_brush_types::TerrainBrushKind;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub enum ConstructionTool {
    #[default]
    Inspect,
    Placement,
    Biome,
    TankEdit,
    FencePlacement,
    PathPlacement,
    ElevatedPathPlacement,
    ElevatedCurbPlacement,
    SkyTrackPlacement(Option<AssetId>),
    GroundTrackPlacement(Option<AssetId>),
    Place(AssetId),
    Fence(AssetId),
    Path(AssetId),
    Terrain(TerrainBrushKind),
    Delete,
}

/// Player-authored placement policy shared by the active construction tool.
/// This is bounded global policy, not a copy of placed objects or terrain.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructionPlacementPolicy {
    pub repeat_placement: bool,
    pub selected_biome: AssetId,
    pub biome_surface: BiomeSurface,
    pub biome_paint_trees: bool,
    pub biome_paint_foliage: bool,
    pub biome_paint_rocks: bool,
    pub biome_automatic_placement_variation: BiomeAutomaticPlacementVariationKind,
    pub terrain_flatten_height_cm: i16,
    pub fence_shape: FencePlacementShape,
    pub fence_heading_eighth_turns: u8,
    pub elevated_path_grade: ElevatedPathGrade,
    pub elevated_path_height_steps: i16,
    pub sky_tower_height_steps: i16,
    pub tank_edit: TankEditKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FencePlacementShape {
    #[default]
    Freeform,
    Rectangle,
    Octagon,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ElevatedPathGrade {
    #[default]
    Flat,
    RampUp,
    RampDown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TankEditKind {
    #[default]
    RaiseWall,
    LowerWall,
    RaiseFloor,
    LowerFloor,
    Fill,
    Drain,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BiomeSurface {
    DeepWater,
    ShallowWater,
    #[default]
    Ground,
    MixedGround,
    GroundCover,
    FoliageMix,
}

impl Default for ConstructionPlacementPolicy {
    fn default() -> Self {
        Self {
            repeat_placement: true,
            selected_biome: AssetId::from_key("grassland"),
            biome_surface: BiomeSurface::Ground,
            biome_paint_trees: false,
            biome_paint_foliage: false,
            biome_paint_rocks: false,
            biome_automatic_placement_variation: BiomeAutomaticPlacementVariationKind::FoliageMix,
            terrain_flatten_height_cm: 0,
            fence_shape: FencePlacementShape::Freeform,
            fence_heading_eighth_turns: 0,
            elevated_path_grade: ElevatedPathGrade::Flat,
            elevated_path_height_steps: 0,
            sky_tower_height_steps: 0,
            tank_edit: TankEditKind::RaiseWall,
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SelectConstructionTool(pub ConstructionTool);
