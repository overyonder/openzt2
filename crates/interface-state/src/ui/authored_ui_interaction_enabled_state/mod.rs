use bevy::prelude::*;

/// Authoritative interaction gate for a projected UI node. Unlike keyboard
/// focus, every actionable node has this component, including pointer-only
/// controls.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiInteractionEnabled(pub bool);
