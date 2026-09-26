use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SimulationControl {
    pub(crate) speed_tier: u8,
    pub(crate) paused: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetSimulationSpeed {
    pub(crate) tier: u8,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetSimulationPaused(pub(crate) bool);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SimulationControlChanged {
    pub(crate) speed_tier: u8,
    pub(crate) paused: bool,
}
