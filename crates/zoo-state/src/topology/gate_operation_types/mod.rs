use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GateOperationSet;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    pub open: bool,
    pub locked: bool,
}

/// Only the polling clock is live mechanism state. Policy is borrowed from
/// the canonical fence definition; animation playback owns the gate's pose.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GateMechanism {
    pub elapsed_poll_time: std::time::Duration,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateAutoClose(pub u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetGateState {
    pub gate: Entity,
    pub open: bool,
}
