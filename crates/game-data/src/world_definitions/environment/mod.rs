//! Authored environment, weather, lighting, fog, sky, and ambient-spawn definitions.

use crate::AssetId;
mod environment_visual_flag_operations;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum EnvironmentFogQuality {
    Low,
    Medium,
    High,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct EnvironmentFogSample {
    pub quality: EnvironmentFogQuality,
    pub day_fraction: f32,
    pub start_factor: f32,
    pub end_factor: f32,
    pub color_unorm: [u16; 3],
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct EnvironmentMapSample {
    pub day_fraction: f32,
    pub water_texture: AssetId,
    pub object_texture: AssetId,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum EnvironmentVisualKind {
    Sky,
    Sun,
    Skirt,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct EnvironmentVisualFlags(u8);

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct EnvironmentVisualSample {
    pub kind: EnvironmentVisualKind,
    pub layer: u16,
    pub order: u16,
    pub day_fraction: f32,
    pub asset: AssetId,
    pub position_model: AssetId,
    pub node: AssetId,
    pub scale: f32,
    pub color_unorm: [u16; 3],
    pub flags: EnvironmentVisualFlags,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum EnvironmentLightTarget {
    Terrain,
    Object,
    Water,
    Cloud,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum EnvironmentLightKind {
    Ambient,
    Sun,
    Side,
    Back,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct EnvironmentLightSample {
    pub target: EnvironmentLightTarget,
    pub kind: EnvironmentLightKind,
    pub quality: u8,
    pub key: AssetId,
    pub model: AssetId,
    pub day_fraction: f32,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    pub direction: [f32; 3],
    pub intensity: f32,
    pub shadow: f32,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct EnvironmentDefinition {
    pub id: AssetId,
    pub family: AssetId,
    pub light_keyframes: Vec<EnvironmentLightKeyframe>,
    pub fog_keyframes: Vec<EnvironmentFogKeyframe>,
    pub sky_keyframes: Vec<EnvironmentSkyKeyframe>,
    pub weather: Vec<AssetId>,
    pub transitions: Vec<WeatherTransitionRule>,
    pub ambient: Vec<AssetId>,
    pub fog_samples: Vec<EnvironmentFogSample>,
    pub map_samples: Vec<EnvironmentMapSample>,
    pub visual_samples: Vec<EnvironmentVisualSample>,
    pub light_samples: Vec<EnvironmentLightSample>,
    pub initial_weather: AssetId,
    pub wind_mps: [f32; 2],
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct EnvironmentLightKeyframe {
    pub day_fraction: u16,
    pub direction_snorm: [i16; 3],
    pub color_unorm: [u16; 3],
    pub illuminance_lux: f32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct EnvironmentFogKeyframe {
    pub day_fraction: u16,
    pub color_unorm: [u16; 3],
    pub start_cm: u32,
    pub end_cm: u32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct EnvironmentSkyKeyframe {
    pub day_fraction: u16,
    pub texture: AssetId,
    pub tint_unorm: [u16; 4],
    pub rotation_snorm: i16,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WeatherDefinition {
    pub id: AssetId,
    pub environment: AssetId,
    pub duration_ticks: [u32; 2],
    pub wind_mps: [f32; 2],
    pub light_multiplier: u16,
    pub fog_multiplier: u16,
    pub audio: AssetId,
    pub effect_asset: AssetId,
    pub effect_emitter: AssetId,
    pub welfare_delta: i16,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct WeatherTransitionRule {
    pub from: AssetId,
    pub to: AssetId,
    pub probability: u32,
    pub transition_ticks: u32,
    pub day_fraction: [u16; 2],
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum AmbientClass {
    Air,
    Ground,
    Water,
}
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct AmbientSpawnDefinition {
    pub id: AssetId,
    pub environment: AssetId,
    pub prefabs: Vec<AssetId>,
    pub biomes: Vec<AssetId>,
    pub class: AmbientClass,
    pub day_fraction: [u16; 2],
    pub population: [u16; 2],
    pub spawn_interval_ticks: [u32; 2],
    pub radius_cm: [u32; 2],
    pub lifetime_ticks: [u32; 2],
    pub probability: u32,
    pub audio: AssetId,
    pub effect_asset: AssetId,
    pub effect_emitter: AssetId,
}
