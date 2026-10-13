use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GateOperationSet;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) open: bool,
    pub(crate) locked: bool,
}

/// Only the polling clock is live mechanism state. Policy is borrowed from
/// the canonical fence definition; animation playback owns the gate's pose.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateMechanism {
    pub(crate) elapsed_poll_time: std::time::Duration,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateAutoClose(pub(crate) u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetGateState {
    pub(crate) gate: Entity,
    pub(crate) open: bool,
}
