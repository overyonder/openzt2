use bevy::prelude::*;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;

pub const TERRAIN_WATER_RENDER_LAYER: usize = 3;
pub const WATER_BUMP_COMBINATION_RENDER_LAYER: usize = 4;
pub const WATER_RENDER_TARGET_SIDE: u32 = 512;
pub const WATER_REFLECTION_TARGET_SIZE: UVec2 = UVec2::new(512, 256);
pub const WATER_REFRACTION_TARGET_SIZE: UVec2 = UVec2::new(1024, 512);

#[derive(Resource)]
pub struct AuthoredTerrainWaterRendererTargets {
    pub combined_bump_map: Handle<Image>,
    pub reflection: Handle<Image>,
    pub refraction: Handle<Image>,
    pub fallback_reflection: Handle<Image>,
    pub bump_camera: Entity,
    pub reflection_camera: Entity,
    pub refraction_camera: Entity,
    pub fallback_reflection_camera: Entity,
    pub water_bump_combination_materials: Box<[Handle<EffectPassMaterial>]>,
    pub elapsed_seconds: f32,
    pub active_water_plane_height: Option<f32>,
    pub reflection_world_extent: Vec2,
    pub refraction_world_extent: Vec2,
    pub fallback_reflection_world_extent: Vec2,
}

#[derive(Component, Default)]
pub struct TerrainWaterScumScroll {
    pub offsets: Vec4,
}

#[derive(Component)]
pub struct TerrainWaterSurfaceRegion(pub Entity);

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainWaterWaveStateKey(pub Entity);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum TerrainWaterSurfacePassKind {
    Refraction,
    Reflection,
    FallbackReflection,
    Scum,
    LowDetail,
    MediumDetail,
    MediumEnvironment,
}

impl TerrainWaterSurfacePassKind {
    pub const fn uses_stitch_geometry(self) -> bool {
        matches!(
            self,
            Self::LowDetail | Self::MediumDetail | Self::MediumEnvironment
        )
    }
}

#[derive(Component)]
pub struct AuthoredTerrainWaterSurfaceEffectPass {
    pub surface_plane_height: f32,
    pub horizontal_minimum: Vec2,
    pub horizontal_maximum: Vec2,
    pub maximum_absolute_input_height: f32,
}

#[derive(Component)]
pub struct AuthoredTerrainWaterfallDecalEffectPass {
    pub material: Handle<EffectPassMaterial>,
    pub rotate_detail: bool,
    pub detail_vertical_scroll_per_second: f32,
}

#[derive(Component)]
pub struct TerrainWaterLowDetailTextureState {
    pub start_seconds: [Option<f32>; 2],
    pub reverse: [bool; 2],
    pub transforms: [Mat4; 2],
}

impl Default for TerrainWaterLowDetailTextureState {
    fn default() -> Self {
        Self {
            start_seconds: [None; 2],
            reverse: [false; 2],
            transforms: [Mat4::IDENTITY; 2],
        }
    }
}
