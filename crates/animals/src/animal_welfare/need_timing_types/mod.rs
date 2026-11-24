use std::time::Duration;

use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component)]
pub struct NeedAdjustmentCadence {
    timer: Timer,
    random: DeterministicRng,
}

impl NeedAdjustmentCadence {
    #[must_use]
    pub fn from_saved_remaining(remaining_ns: u64, random: DeterministicRng) -> Self {
        Self {
            timer: Timer::new(Duration::from_nanos(remaining_ns), TimerMode::Once),
            random,
        }
    }

    #[must_use]
    pub fn saved_remaining(&self) -> (u64, [u64; 2]) {
        let remaining = u64::try_from(self.timer.remaining().as_nanos()).unwrap_or(u64::MAX);
        (remaining, self.random.to_raw())
    }

    pub(crate) fn tick(&mut self, step: Duration, delay_steps: u32) -> bool {
        openzt2_zoo_state::need_adjustment::tick_need_adjustment(&mut self.timer, step, delay_steps, &mut self.random)
    }
}
