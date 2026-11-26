use std::time::Duration;

use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

#[inline]
pub(super) fn convert_authored_nanoseconds_to_fixed_simulation_ticks(
    authored_nanoseconds: u64,
    world_definitions: WorldDefinitionsView<'_>,
) -> u32 {
    world_definitions.simulation_timestep().map_or(0, |step| {
        u32::try_from(fixed_ticks(
            Duration::from_nanos(authored_nanoseconds),
            step,
        ))
        .unwrap_or(u32::MAX)
    })
}

pub(super) fn fixed_ticks(duration: Duration, step: Duration) -> u64 {
    u64::try_from(duration.as_nanos().div_ceil(step.as_nanos())).unwrap_or(u64::MAX)
}

pub(super) fn fixed_ticks_for_seconds(seconds: f32, step: Duration) -> u64 {
    Duration::try_from_secs_f32(seconds).map_or(0, |duration| fixed_ticks(duration, step))
}

#[derive(Component)]
pub struct RunningShowPhaseTimer(pub(super) Timer);

impl RunningShowPhaseTimer {
    pub(super) fn new(step: Duration, ticks: u32) -> Self {
        Self::from_remaining(step.saturating_mul(ticks))
    }

    pub fn from_remaining(remaining: Duration) -> Self {
        Self(Timer::new(remaining, TimerMode::Once))
    }

    pub fn remaining(&self) -> Duration {
        self.0.remaining()
    }

    pub fn remaining_ticks(&self, step: Duration) -> u32 {
        u32::try_from(fixed_ticks(self.remaining(), step)).unwrap_or(u32::MAX)
    }
}
