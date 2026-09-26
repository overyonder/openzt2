use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Clone, Copy)]
pub(super) struct AmbientAnimalSpawner {
    pub(super) definition: AssetId,
    pub(super) next_tick: u64,
    pub(super) random: DeterministicRng,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AmbientAnimal {
    pub(super) definition: AssetId,
    pub(super) despawn_tick: u64,
}
