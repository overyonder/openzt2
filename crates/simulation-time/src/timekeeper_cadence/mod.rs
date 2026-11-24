use bevy::prelude::*;
use std::time::Duration;

use openzt2_world_state::{
    simulation_time::simulation_clock_types::{ZooHeartbeatElapsed, ZooSlowTickElapsed},
    world_spawn::world_membership_types::WorldRoot,
};

#[derive(Component)]
pub(super) struct ZooSlowTickTimer(Timer);

#[derive(Component)]
pub(super) struct ZooHeartbeatTimer(Timer);

impl ZooSlowTickTimer {
    pub(super) fn new(interval: Duration) -> Self {
        Self(Timer::new(interval, TimerMode::Repeating))
    }
}

impl ZooHeartbeatTimer {
    pub(super) fn new(interval: Duration, initially_paused: bool) -> Self {
        let mut timer = Timer::new(interval, TimerMode::Repeating);
        if initially_paused {
            timer.pause();
        }
        Self(timer)
    }
}

pub(super) fn advance_slow_tick(
    fixed_time: Res<Time<Fixed>>,
    mut roots: Query<(Entity, &mut ZooSlowTickTimer), With<WorldRoot>>,
    mut elapsed: MessageWriter<ZooSlowTickElapsed>,
) {
    let Ok((root, mut cadence)) = roots.single_mut() else {
        return;
    };
    let count = cadence
        .0
        .tick(fixed_time.delta())
        .times_finished_this_tick();
    if count != 0 {
        elapsed.write(ZooSlowTickElapsed { root, count });
    }
}

pub(super) fn advance_heartbeat(
    real_time: Res<Time<Real>>,
    mut roots: Query<(Entity, &mut ZooHeartbeatTimer), With<WorldRoot>>,
    mut elapsed: MessageWriter<ZooHeartbeatElapsed>,
) {
    let Ok((root, mut cadence)) = roots.single_mut() else {
        return;
    };
    if cadence.0.is_paused() {
        return;
    }
    let count = cadence.0.tick(real_time.delta()).times_finished_this_tick();
    if count != 0 {
        elapsed.write(ZooHeartbeatElapsed { root, count });
    }
}
