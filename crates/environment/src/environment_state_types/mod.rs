use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct Daylight {
    pub(super) fraction: f32,
}

#[derive(Component, Clone, Copy)]
pub(super) struct EnvironmentRandom(pub(super) DeterministicRng);

/// Active authored quake presentation on the world environment entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Earthquake {
    pub(super) refresh_generation: u32,
}
