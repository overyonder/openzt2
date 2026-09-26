use super::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use bevy::prelude::*;

pub(crate) const TERRAIN_WATER_RENDER_LAYER: usize = 3;
pub(super) const WATER_BUMP_COMBINATION_RENDER_LAYER: usize = 4;
pub(super) const WATER_RENDER_TARGET_SIDE: u32 = 512;

#[derive(Resource)]
pub(super) struct AuthoredTerrainWaterRendererTargets {
    pub(super) combined_bump_map: Handle<Image>,
    pub(super) reflection: Handle<Image>,
    pub(super) refraction: Handle<Image>,
    pub(super) bump_camera: Entity,
    pub(super) reflection_camera: Entity,
    pub(super) refraction_camera: Entity,
    pub(super) water_bump_combination_materials: Box<[Handle<EffectPassMaterial>]>,
    pub(super) elapsed_seconds: f32,
    pub(super) active_water_plane_height: Option<f32>,
    pub(super) wave_states: bevy::platform::collections::HashMap<
        TerrainWaterWaveStateKey,
        NativeTerrainWaterGeometricWaveShaderState,
    >,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TerrainWaterWaveStateKey(
    pub(super) bevy::asset::AssetId<TerrainAsset>,
    pub(super) u8,
);

#[derive(Component)]
pub(crate) struct AuthoredTerrainWaterSurfaceEffectPass {
    pub(crate) surface_plane_height: f32,
    pub(crate) horizontal_minimum: Vec2,
    pub(crate) horizontal_maximum: Vec2,
    pub(super) maximum_absolute_input_height: f32,
}

#[derive(Component)]
pub(super) struct AuthoredTerrainWaterfallDecalEffectPass {
    pub(super) material: Handle<EffectPassMaterial>,
    pub(super) rotate_detail: bool,
    pub(super) detail_vertical_scroll_per_second: f32,
}
