use bevy::prelude::*;

/// Canonical lower bound for one authored integer-valued UI control.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiMinimum(pub(crate) i64);

/// Canonical upper bound for one authored integer-valued UI control.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiMaximum(pub(crate) i64);
