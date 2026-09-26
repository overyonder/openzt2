use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind, AssetId,
};

use crate::plugins::terrain::terrain_brush_types::TerrainBrushKind;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum ConstructionTool {
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
pub(crate) struct ConstructionPlacementPolicy {
    pub(crate) repeat_placement: bool,
    pub(crate) selected_biome: AssetId,
    pub(crate) biome_surface: BiomeSurface,
    pub(crate) biome_paint_trees: bool,
    pub(crate) biome_paint_foliage: bool,
    pub(crate) biome_paint_rocks: bool,
    pub(crate) biome_automatic_placement_variation: BiomeAutomaticPlacementVariationKind,
    pub(crate) biome_texture_brush_radius_half_metre_steps: u8,
    pub(crate) biome_water_brush_radius_half_metre_steps: u8,
    pub(crate) terrain_deformation_brush_radius_half_metre_steps: u8,
    pub(crate) terrain_flatten_height_cm: i16,
    pub(crate) terrain_flatten_strength_tenths_per_second: u16,
    pub(crate) fence_shape: FencePlacementShape,
    pub(crate) elevated_path_grade: ElevatedPathGrade,
    pub(crate) elevated_path_height_steps: i16,
    pub(crate) sky_tower_height_steps: i16,
    pub(crate) tank_edit: TankEditKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FencePlacementShape {
    #[default]
    Freeform,
    Rectangle,
    Octagon,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ElevatedPathGrade {
    #[default]
    Flat,
    RampUp,
    RampDown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TankEditKind {
    #[default]
    RaiseWall,
    LowerWall,
    RaiseFloor,
    LowerFloor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BiomeSurface {
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
            biome_texture_brush_radius_half_metre_steps: 10,
            biome_water_brush_radius_half_metre_steps: 10,
            terrain_deformation_brush_radius_half_metre_steps: 10,
            terrain_flatten_height_cm: 0,
            terrain_flatten_strength_tenths_per_second: 30,
            fence_shape: FencePlacementShape::Freeform,
            elevated_path_grade: ElevatedPathGrade::Flat,
            elevated_path_height_steps: 0,
            sky_tower_height_steps: 0,
            tank_edit: TankEditKind::RaiseWall,
        }
    }
}

impl ConstructionPlacementPolicy {
    pub(crate) fn selected_terrain_brush_strength_metres_per_second(
        self,
        selected_tool: ConstructionTool,
    ) -> Option<f32> {
        match selected_tool {
            ConstructionTool::Terrain(TerrainBrushKind::Raise | TerrainBrushKind::Lower) => {
                Some(5.0)
            }
            ConstructionTool::Terrain(TerrainBrushKind::Smooth) => Some(2.0),
            ConstructionTool::Terrain(TerrainBrushKind::Flatten { .. }) => {
                Some(f32::from(self.terrain_flatten_strength_tenths_per_second) * 0.1)
            }
            ConstructionTool::Terrain(
                TerrainBrushKind::Paint { .. }
                | TerrainBrushKind::PaintWater { .. }
                | TerrainBrushKind::AddWater
                | TerrainBrushKind::RemoveWater,
            ) => Some(1.0),
            _ => None,
        }
    }

    pub(crate) fn selected_terrain_brush_radius_metres(
        self,
        selected_tool: ConstructionTool,
    ) -> Option<f32> {
        let half_metre_steps = match selected_tool {
            ConstructionTool::Biome | ConstructionTool::Terrain(TerrainBrushKind::Paint { .. }) => {
                self.biome_texture_brush_radius_half_metre_steps
            }
            ConstructionTool::Terrain(
                TerrainBrushKind::AddWater
                | TerrainBrushKind::PaintWater { .. }
                | TerrainBrushKind::RemoveWater,
            ) => self.biome_water_brush_radius_half_metre_steps,
            ConstructionTool::Terrain(
                TerrainBrushKind::Raise
                | TerrainBrushKind::Lower
                | TerrainBrushKind::Smooth
                | TerrainBrushKind::Flatten { .. },
            ) => self.terrain_deformation_brush_radius_half_metre_steps,
            _ => return None,
        };
        Some(f32::from(half_metre_steps) * 0.5)
    }

    pub(crate) fn adjust_selected_terrain_brush_radius_by_authored_half_metre_step(
        &mut self,
        selected_tool: ConstructionTool,
        direction: i32,
    ) -> Option<f32> {
        let (half_metre_steps, minimum) = match selected_tool {
            ConstructionTool::Biome | ConstructionTool::Terrain(TerrainBrushKind::Paint { .. }) => {
                (&mut self.biome_texture_brush_radius_half_metre_steps, 2)
            }
            ConstructionTool::Terrain(
                TerrainBrushKind::AddWater
                | TerrainBrushKind::PaintWater { .. }
                | TerrainBrushKind::RemoveWater,
            ) => (&mut self.biome_water_brush_radius_half_metre_steps, 8),
            ConstructionTool::Terrain(
                TerrainBrushKind::Raise
                | TerrainBrushKind::Lower
                | TerrainBrushKind::Smooth
                | TerrainBrushKind::Flatten { .. },
            ) => (
                &mut self.terrain_deformation_brush_radius_half_metre_steps,
                6,
            ),
            _ => return None,
        };
        *half_metre_steps = match direction.signum() {
            -1 => half_metre_steps.saturating_sub(1),
            1 => half_metre_steps.saturating_add(1),
            _ => *half_metre_steps,
        }
        .clamp(minimum, 16);
        Some(f32::from(*half_metre_steps) * 0.5)
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct SelectConstructionTool(pub(crate) ConstructionTool);
