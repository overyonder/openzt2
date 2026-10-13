use bevy::prelude::*;
use openzt2_game_data::AssetId;

mod wind_shader_state_operations;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldEnvironment {
    pub definition: AssetId,
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
    direction_envelope: WindEnvelopeInterpolator,
    vibration_envelope: WindEnvelopeInterpolator,
    direction_randomness: f32,
    vibration_phase: f32,
    texture_phases: Vec4,
    random_state: u32,
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
}
