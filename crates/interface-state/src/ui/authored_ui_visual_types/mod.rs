use bevy::prelude::*;
use openzt2_game_data::ui_document::node_presentation::UiNodeVisualState;

/// Interaction state which makes this visual layer visible.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiVisualLayer(pub UiNodeVisualState);

/// Decorative skin nodes borrow the background slot of their owning control.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiVisualBackgroundSlotOwner(pub Entity);

#[derive(Component, Debug, Clone)]
pub struct UiVisualSound(pub Handle<AudioSource>);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiVisualTextColor(pub Color);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiVisualImageColor(pub Color);

/// Selects whether this authored visual participates in pointer picking.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiVisualPicking(pub bool);

/// Source texture rectangle retained only where later presentation changes
/// replace the projected image while preserving its authored sampling bounds.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSourceRect(pub [i32; 4]);
