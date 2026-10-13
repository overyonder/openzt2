use bevy::prelude::*;

/// Canonical lower bound for one authored integer-valued UI control.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiMinimum(pub i64);

/// Canonical upper bound for one authored integer-valued UI control.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiMaximum(pub i64);
