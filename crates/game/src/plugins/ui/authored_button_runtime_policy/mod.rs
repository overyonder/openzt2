use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Runtime facts retained from one authored button definition and consumed by
/// activation, selection, repetition, and composite-button presentation.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiButtonPolicy {
    pub(crate) auto_size: bool,
    pub(crate) minimum_height: i32,
    pub(crate) selectable: bool,
    pub(crate) sticky: bool,
    pub(crate) repress: bool,
    pub(crate) repeat_delay_seconds: f32,
    pub(crate) hold_change: f32,
    pub(crate) hold_interval_cap_seconds: f32,
    pub(crate) activation_data: AssetId,
    pub(crate) child_button: AssetId,
    pub(crate) hover_child: AssetId,
}
