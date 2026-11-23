use bevy::prelude::*;

use crate::animal_welfare::need_timing_types::NeedAdjustmentCadence;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component)]
pub struct EggIncubationState {
    pub(super) amount: f32,
    pub(super) cadence: NeedAdjustmentCadence,
}

impl EggIncubationState {
    #[must_use]
    pub fn from_saved_remaining(amount: f32, remaining_ns: u64, random: DeterministicRng) -> Self {
        Self {
            amount,
            cadence: NeedAdjustmentCadence::from_saved_remaining(remaining_ns, random),
        }
    }

    #[must_use]
    pub fn saved_remaining(&self) -> (f32, u64, [u64; 2]) {
        let (remaining, random) = self.cadence.saved_remaining();
        (self.amount, remaining, random)
    }
}
