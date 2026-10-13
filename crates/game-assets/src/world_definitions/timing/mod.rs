//! Resolution of authored timer policy into the one integer simulation clock.

use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationSpeedMultiplier,
    SimulationTimingDefinition,
};

const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const SECONDS_PER_DAY: f64 = 86_400.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AuthoredTimekeeperEvent {
    pub(super) kind: AuthoredTimekeeperEventKind,
    pub(super) interval_seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuthoredTimekeeperEventKind {
    Heartbeat,
    Simulation,
    Slow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuthoredTimekeeperLoweringError {
    MissingHeartbeat,
    MissingSimulationTick,
    MissingSlowTick,
    DuplicateEvent,
    InvalidInterval,
    InvalidTimeScale,
    UnrepresentableFixedRate,
    UnrepresentableZooDay,
}

/// Converts BFTimeKeeper intervals to nanoseconds and simulation ticks.
///
/// Source clock selection and timer objects end at this lowering boundary.
/// The live world-definition asset retains only the integer fixed-step
/// contract consumed by Bevy's `FixedUpdate` schedule.
pub(super) fn lower_authored_timekeeper_to_simulation_timing_definition(
    time_scale: f64,
    authored_events: &[AuthoredTimekeeperEvent],
) -> Result<SimulationTimingDefinition, AuthoredTimekeeperLoweringError> {
    if !time_scale.is_finite() || time_scale <= 0.0 {
        return Err(AuthoredTimekeeperLoweringError::InvalidTimeScale);
    }

    let event = |wanted| {
        let mut matches = authored_events.iter().filter(|event| event.kind == wanted);
        matches
            .next()
            .ok_or(match wanted {
                AuthoredTimekeeperEventKind::Heartbeat => {
                    AuthoredTimekeeperLoweringError::MissingHeartbeat
                }
                AuthoredTimekeeperEventKind::Simulation => {
                    AuthoredTimekeeperLoweringError::MissingSimulationTick
                }
                AuthoredTimekeeperEventKind::Slow => {
                    AuthoredTimekeeperLoweringError::MissingSlowTick
                }
            })
            .and_then(|event| {
                matches
                    .next()
                    .is_none()
                    .then_some(event)
                    .ok_or(AuthoredTimekeeperLoweringError::DuplicateEvent)
            })
    };

    let heartbeat = event(AuthoredTimekeeperEventKind::Heartbeat)?;
    let simulation = event(AuthoredTimekeeperEventKind::Simulation)?;
    let slow = event(AuthoredTimekeeperEventKind::Slow)?;
    if [heartbeat, simulation, slow].into_iter().any(|event| {
        !event.interval_seconds.is_finite()
            || event.interval_seconds <= 0.0
            || event.interval_seconds * NANOS_PER_SECOND < 1.0
    }) {
        return Err(AuthoredTimekeeperLoweringError::InvalidInterval);
    }

    let fixed_hz = (1.0 / simulation.interval_seconds).round();
    if !(1.0..=u16::MAX as f64).contains(&fixed_hz) {
        return Err(AuthoredTimekeeperLoweringError::UnrepresentableFixedRate);
    }
    let ticks_per_day = (SECONDS_PER_DAY / (simulation.interval_seconds * time_scale)).round();
    if !(1.0..=u32::MAX as f64).contains(&ticks_per_day) {
        return Err(AuthoredTimekeeperLoweringError::UnrepresentableZooDay);
    }

    let speed_multipliers = vec![SimulationSpeedMultiplier {
        numerator: 1,
        denominator: 1,
    }];
    Ok(SimulationTimingDefinition {
        fixed_hz: fixed_hz as u16,
        ticks_per_day: ticks_per_day as u32,
        speed_multipliers,
        calendar: SimulationCalendarPolicy {
            epoch_year: 1,
            epoch_month: 1,
            epoch_day: 1,
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
            leap: SimulationCalendarLeapPolicy::Gregorian,
        },
    })
}
