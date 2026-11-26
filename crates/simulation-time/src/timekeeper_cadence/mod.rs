use bevy::prelude::*;
use openzt2_game_assets::world_definitions::timing::{
    AuthoredTimekeeperPolicy, TimekeeperEvent, TimekeeperEventTiming,
};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooTimekeeperElapsed,
    world_spawn::world_membership_types::WorldRoot,
};
use std::time::Duration;

#[derive(Component)]
pub(super) struct TimekeeperCadences {
    order: [TimekeeperEvent; 3],
    simulation: Timer,
    slow: Timer,
    heartbeat: Timer,
    heartbeat_frame: HeartbeatFrame,
}

#[derive(Default)]
struct HeartbeatFrame {
    virtual_start: Duration,
    virtual_span: Duration,
    real_span: Duration,
    advanced: Duration,
}

impl TimekeeperCadences {
    pub(super) fn new(policy: AuthoredTimekeeperPolicy) -> Self {
        Self {
            order: policy.event_order,
            simulation: timer(policy.simulation),
            slow: timer(policy.slow),
            heartbeat: timer(policy.heartbeat),
            heartbeat_frame: HeartbeatFrame::default(),
        }
    }

    pub(super) fn fixed_pulses(
        &mut self,
        step: Duration,
        fixed_elapsed: Duration,
        paused: bool,
    ) -> impl Iterator<Item = (TimekeeperEvent, u32)> {
        let simulation = tick_game_clock(&mut self.simulation, step, paused);
        let slow = tick_game_clock(&mut self.slow, step, paused);
        // Consume real time only through this fixed boundary.
        let frame = &mut self.heartbeat_frame;
        let covered = fixed_elapsed
            .saturating_sub(frame.virtual_start)
            .min(frame.virtual_span);
        let real_nanos = frame
            .real_span
            .as_nanos()
            .saturating_mul(covered.as_nanos())
            .checked_div(frame.virtual_span.as_nanos())
            .unwrap_or(0);
        let through = Duration::from_nanos(u64::try_from(real_nanos).unwrap_or(u64::MAX));
        let delta = through.saturating_sub(frame.advanced);
        frame.advanced = through;
        let heartbeat = tick_heartbeat(&mut self.heartbeat, delta);
        self.order.into_iter().filter_map(move |event| {
            let count = match event {
                TimekeeperEvent::Heartbeat => heartbeat,
                TimekeeperEvent::Simulation => simulation,
                TimekeeperEvent::Slow => slow,
            };
            (count != 0).then_some((event, count))
        })
    }
}

fn tick_heartbeat(timer: &mut Timer, delta: Duration) -> u32 {
    if timer.is_paused() || delta.is_zero() {
        0
    } else {
        timer.tick(delta).times_finished_this_tick()
    }
}

fn timer(timing: TimekeeperEventTiming) -> Timer {
    let mut timer = Timer::new(timing.interval, TimerMode::Repeating);
    if timing.initially_paused {
        timer.pause();
    }
    timer
}

fn tick_game_clock(timer: &mut Timer, step: Duration, paused: bool) -> u32 {
    if paused {
        timer.pause();
        return 0;
    }
    timer.unpause();
    timer.tick(step).times_finished_this_tick()
}

pub(super) fn slow_tick_elapsed(mut events: MessageReader<ZooTimekeeperElapsed>) -> bool {
    let due = events
        .read()
        .any(|pulse| pulse.event == TimekeeperEvent::Slow);
    events.clear();
    due
}

pub(super) fn heartbeat_elapsed(mut events: MessageReader<ZooTimekeeperElapsed>) -> bool {
    let due = events
        .read()
        .any(|pulse| pulse.event == TimekeeperEvent::Heartbeat);
    events.clear();
    due
}

pub(super) fn sample_heartbeat_frame(
    real_time: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
    mut roots: Query<&mut TimekeeperCadences, With<WorldRoot>>,
) {
    let Ok(mut cadence) = roots.single_mut() else {
        return;
    };
    cadence.heartbeat_frame = HeartbeatFrame {
        virtual_start: virtual_time.elapsed().saturating_sub(virtual_time.delta()),
        virtual_span: virtual_time.delta(),
        real_span: real_time.delta(),
        advanced: Duration::ZERO,
    };
}

pub(super) fn finish_heartbeat_frame(
    mut roots: Query<(Entity, &mut TimekeeperCadences), With<WorldRoot>>,
    mut events: MessageWriter<ZooTimekeeperElapsed>,
) {
    let Ok((root, mut cadence)) = roots.single_mut() else {
        return;
    };
    let frame = &cadence.heartbeat_frame;
    let remaining = frame.real_span.saturating_sub(frame.advanced);
    let count = tick_heartbeat(&mut cadence.heartbeat, remaining);
    if count != 0 {
        events.write(ZooTimekeeperElapsed {
            root,
            event: TimekeeperEvent::Heartbeat,
            count,
        });
    }
}
