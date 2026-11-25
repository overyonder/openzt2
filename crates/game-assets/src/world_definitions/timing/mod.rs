//! Authored timekeeper cadence and pause policy.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimekeeperEventTiming {
    pub interval: Duration,
    pub initially_paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimekeeperEvent {
    Heartbeat,
    Simulation,
    Slow,
}

impl TimekeeperEvent {
    pub(super) const fn source_name(self) -> &'static str {
        match self {
            Self::Heartbeat => "BFTIME_HEARTBEAT",
            Self::Simulation => "BFTIME_SIMTICK",
            Self::Slow => "BFTIME_SLOWTICK",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredTimekeeperPolicy {
    pub heartbeat: TimekeeperEventTiming,
    pub simulation: TimekeeperEventTiming,
    pub slow: TimekeeperEventTiming,
    pub event_order: [TimekeeperEvent; 3],
    pub time_scale: f64,
}
