use crate::terrain::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use bevy::prelude::*;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

pub const TERRAIN_WATER_RENDER_LAYER: usize = 3;
pub const WATER_BUMP_COMBINATION_RENDER_LAYER: usize = 4;
pub const WATER_RENDER_TARGET_SIDE: u32 = 512;

#[derive(Resource)]
pub struct AuthoredTerrainWaterRendererTargets {
    pub combined_bump_map: Handle<Image>,
    pub reflection: Handle<Image>,
    pub refraction: Handle<Image>,
    pub bump_camera: Entity,
    pub reflection_camera: Entity,
    pub refraction_camera: Entity,
    pub water_bump_combination_materials: Box<[Handle<EffectPassMaterial>]>,
    pub elapsed_seconds: f32,
    pub active_water_plane_height: Option<f32>,
    pub wave_states: bevy::platform::collections::HashMap<
        TerrainWaterWaveStateKey,
        NativeTerrainWaterGeometricWaveShaderState,
    >,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainWaterWaveStateKey(pub bevy::asset::AssetId<TerrainAsset>, pub u8);

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
