use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Runtime facts retained from one authored button definition and consumed by
/// activation, selection, repetition, and composite-button presentation.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiButtonPolicy {
    pub auto_size: bool,
    pub minimum_height: i32,
    pub selectable: bool,
    pub sticky: bool,
    pub repress: bool,
    pub repeat_delay_seconds: f32,
    pub hold_change: f32,
    pub hold_interval_cap_seconds: f32,
    pub activation_data: AssetId,
    pub child_button: AssetId,
    pub hover_child: AssetId,
}
