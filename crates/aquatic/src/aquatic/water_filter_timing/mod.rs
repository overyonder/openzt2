use bevy::prelude::*;
use std::time::Duration;

#[derive(Component)]
pub struct FilterCleanupInterval(pub(super) Timer);

impl FilterCleanupInterval {
    pub(super) fn from_seconds(seconds: f32) -> Self {
        Self(Timer::new(
            Duration::try_from_secs_f32(seconds).unwrap_or_default(),
            TimerMode::Once,
        ))
    }

    pub fn from_saved_remaining(seconds: f32, remaining: Duration) -> Self {
        let mut interval = Self::from_seconds(seconds);
        interval
            .0
            .set_elapsed(interval.0.duration().saturating_sub(remaining));
        interval
    }

    pub fn remaining(&self) -> Duration {
        self.0.remaining()
    }
}
