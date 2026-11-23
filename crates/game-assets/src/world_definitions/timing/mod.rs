//! Authored timekeeper cadence and pause policy.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimekeeperEventTiming {
    pub interval: Duration,
    pub pauses_with_simulation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredTimekeeperPolicy {
    pub heartbeat: TimekeeperEventTiming,
    pub simulation: TimekeeperEventTiming,
    pub slow: TimekeeperEventTiming,
    pub time_scale: f64,
}
