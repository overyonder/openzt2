use bevy::prelude::*;
use openzt2_game_data::ui_document::node_presentation::UiNodeVisualState;

/// Interaction state which makes this visual layer visible.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiVisualLayer(pub(crate) UiNodeVisualState);

#[derive(Component, Debug, Clone)]
pub(crate) struct UiVisualSound(pub(crate) Handle<AudioSource>);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiVisualTextColor(pub(crate) Color);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiVisualImageColor(pub(crate) Color);

/// Selects whether this authored visual participates in pointer picking.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiVisualPicking(pub(crate) bool);

/// Source texture rectangle retained only where later presentation changes
/// replace the projected image while preserving its authored sampling bounds.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiSourceRect(pub(crate) [i32; 4]);
