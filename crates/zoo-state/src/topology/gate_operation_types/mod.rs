use std::time::Duration;

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GateOperationSet;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    pub open: bool,
    pub locked: bool,
}

#[derive(Component, Debug, Clone)]
pub struct GateMechanism(pub Timer);

impl Default for GateMechanism {
    fn default() -> Self {
        Self(Timer::new(Duration::ZERO, TimerMode::Repeating))
    }
}

#[derive(Component, Debug, Clone)]
pub struct GateAutoClose(pub Timer);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetGateState {
    pub gate: Entity,
    pub open: bool,
}
