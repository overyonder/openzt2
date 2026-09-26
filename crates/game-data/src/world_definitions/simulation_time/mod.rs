//! Authored simulation tick, speed, and calendar definitions.

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum SimulationCalendarLeapPolicy {
    None,
    Gregorian,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct SimulationCalendarPolicy {
    pub epoch_year: u16,
    pub epoch_month: u8,
    pub epoch_day: u8,
    pub month_lengths: [u8; 12],
    pub leap: SimulationCalendarLeapPolicy,
}
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct SimulationTimingDefinition {
    pub fixed_hz: u16,
    pub ticks_per_day: u32,
    pub speed_multipliers: Vec<SimulationSpeedMultiplier>,
    pub calendar: SimulationCalendarPolicy,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct SimulationSpeedMultiplier {
    pub numerator: u16,
    pub denominator: u16,
}
