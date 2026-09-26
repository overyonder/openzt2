use bevy::prelude::*;
use openzt2_game_data::AssetId;

mod wind_shader_state_operations;

use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorldEnvironment {
    pub(super) definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct Daylight {
    pub(super) fraction: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct Wind {
    pub(super) direction: Vec2,
    pub(super) speed_mps: f32,
}

/// Seven-register wind presentation state consumed by `winddistort.h`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct AuthoredWindShaderPresentationState {
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

#[derive(Component, Clone, Copy)]
pub(super) struct EnvironmentRandom(pub(super) DeterministicRng);

/// Active authored quake presentation on the world environment entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Earthquake {
    pub(crate) refresh_generation: u32,
}
