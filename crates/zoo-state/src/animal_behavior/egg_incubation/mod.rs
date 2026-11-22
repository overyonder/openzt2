use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

/// The current incubation and need-adjustment clock of a live egg.
#[derive(Component, Clone, Copy, Debug)]
pub struct EggIncubation {
    pub amount: f32,
    pub remaining_ns: u64,
    pub random: DeterministicRng,
}
