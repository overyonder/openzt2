//! Actor-owned deterministic random state for animal and staff behavior.

use bevy::prelude::*;

use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Clone, Copy)]
pub(crate) struct BehaviorRandomStream(pub(super) DeterministicRng);

impl BehaviorRandomStream {
    pub(crate) fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    pub(crate) fn range_u32(&mut self, upper_bound: u32) -> Option<u32> {
        self.0.range_u32(upper_bound)
    }
}
