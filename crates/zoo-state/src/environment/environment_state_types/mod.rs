use bevy::prelude::*;
use openzt2_game_data::AssetId;

mod wind_shader_state_operations;

/// An authoritative simulation step accepted by the zoo clock.
#[derive(Message, Clone, Copy)]
pub struct ZooTickAdvanced(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldEnvironment {
    pub definition: AssetId,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum EnvironmentMapTexture {
    #[default]
    Unresolved,
    Unauthored,
    Empty,
    Texture(Handle<Image>),
}

#[derive(Component, Default, Clone, Debug, PartialEq)]
pub struct EnvironmentMapSamples {
    pub water: EnvironmentMapTexture,
    pub object: EnvironmentMapTexture,
}

impl EnvironmentMapTexture {
    pub fn texture(&self) -> Option<&Handle<Image>> {
        match self {
            Self::Texture(texture) => Some(texture),
            _ => None,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentWaterRayLight {
    pub raw_object_ambient_rgba: Vec4,
    pub first_object_light_weighted_rgb: Option<Vec3>,
    pub sun_direction: Option<Vec3>,
}

impl EnvironmentWaterRayLight {
    pub fn raw_object_ambient_rgba(&self) -> Vec4 {
        self.raw_object_ambient_rgba
    }

    pub fn object_light_horizontal_vector(&self) -> Option<Vec2> {
        self.first_object_light_weighted_rgb
            .map(|light| (self.raw_object_ambient_rgba.truncate() + light).truncate())
    }

    /// Direction in Bevy world coordinates.
    pub fn sun_direction(&self) -> Option<Vec3> {
        self.sun_direction
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Wind {
    pub direction: Vec2,
    pub speed_mps: f32,
}

/// Seven-register wind presentation state consumed by `winddistort.h`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AuthoredWindShaderPresentationState {
    direction: Vec2,
    speed_mps: f32,
    gustiness: f32,
    direction_envelope: WindEnvelopeInterpolator,
    vibration_envelope: WindEnvelopeInterpolator,
    vibration_phase: f32,
    texture_phases: Vec4,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WindEnvelopeInterpolator {
    elapsed: f32,
    duration: f32,
    attack: f32,
    hold: f32,
    release: f32,
    idle: f32,
    output: f32,
    random_amplitude: f32,
    minimum: f32,
    maximum: f32,
    stage_ranges: [[f32; 2]; 4],
}
