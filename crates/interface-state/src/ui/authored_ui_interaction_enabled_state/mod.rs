use bevy::prelude::*;

/// Authoritative interaction gate for a projected UI node. Unlike keyboard
/// focus, every actionable node has this component, including pointer-only
/// controls.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiInteractionEnabled(pub bool);

/// A domain restriction blocks interaction through this subtree independently
/// of the document and scenario interaction states.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiInteractionRestriction;

/// A scenario's explicit interaction state for this node. A nearer override
/// takes precedence over the inherited authored gate.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiScenarioInteractionOverride(pub bool);
