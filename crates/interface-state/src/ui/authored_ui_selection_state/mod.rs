use bevy::prelude::*;

/// Live selection state for one projected authored UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSelected(pub bool);
