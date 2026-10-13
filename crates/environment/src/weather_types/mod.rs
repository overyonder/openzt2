use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Weather {
    pub(super) definition: AssetId,
    pub(super) elapsed_ticks: u64,
    pub(super) duration_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WeatherTransition {
    pub(super) from: AssetId,
    pub(super) to: AssetId,
    pub(super) start_tick: u64,
    pub(super) duration_ticks: u32,
}

/// A caller operation retained only while its requested weather transition is
/// in flight. Its presence prevents a second request from stealing correlation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingWeatherOperation {
    pub(super) operation: Entity,
    pub(super) definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SetWeather {
    pub(super) operation: Entity,
    pub(super) definition: AssetId,
    pub(super) transition_ticks: Option<u32>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WeatherRequestApplied {
    pub(super) operation: Entity,
    pub(super) definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WeatherRequestRejected {
    pub(super) operation: Entity,
    pub(super) definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WeatherChanged {
    pub(super) previous: AssetId,
    pub(super) current: AssetId,
}
