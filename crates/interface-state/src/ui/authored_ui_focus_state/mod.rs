use bevy::prelude::*;

/// Source-authored keyboard and controller focus policy for one UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiFocusable {
    pub order: u32,
    pub enabled: bool,
}
