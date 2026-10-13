//! Actor-owned deterministic random state for animal and staff behavior.

use bevy::prelude::*;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Clone, Copy)]
pub struct BehaviorRandomStream(pub DeterministicRng);

impl BehaviorRandomStream {
    pub fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    pub fn range_u32(&mut self, upper_bound: u32) -> Option<u32> {
        self.0.range_u32(upper_bound)
    }
}
