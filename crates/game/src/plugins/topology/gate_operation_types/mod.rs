use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) open: bool,
    pub(crate) locked: bool,
}

/// Final source-selected presentation and timing for a gate. The open bit
/// remains on `Gate`; animation state remains owned by Bevy's animator.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateMechanism {
    pub(crate) prefab: AssetId,
    pub(crate) open_animation: AssetId,
    pub(crate) close_animation: AssetId,
    pub(crate) trigger_distance_cm: u16,
    pub(crate) auto_close_ticks: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateAutoClose(pub(crate) u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetGateState {
    pub(crate) gate: Entity,
    pub(crate) open: bool,
}
