use bevy::prelude::*;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::input::input_types::InputChord;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DeviceControlAxes {
    pub(crate) pan: Vec2,
    /// Forward, backward, right, and left magnitudes retained independently.
    pub(crate) pan_directions: [f32; 4],
    pub(crate) look: Vec2,
    pub(crate) zoom: f32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RebindGameActionInput {
    pub(crate) action: GameAction,
    pub(crate) slot: u8,
    pub(crate) chord: InputChord,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameActionInputRebindingRejection {
    InvalidSlot,
    Duplicate { conflicting_action: GameAction },
}
