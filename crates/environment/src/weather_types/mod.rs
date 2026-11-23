use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Weather {
    pub(super) definition: AssetId,
}

#[derive(Component)]
pub(super) struct WeatherDuration(pub(super) Timer);

#[derive(Message)]
pub(super) struct WeatherWelfareDue(pub(super) Entity);

#[derive(Component)]
pub(super) struct WeatherTransition {
    pub(super) to: AssetId,
    pub(super) timer: Timer,
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
