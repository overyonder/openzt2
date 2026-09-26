use super::node_layout::UiNodeRegionDefinition;
use crate::AssetId;
use serde::{Deserialize, Serialize};

/// Source-resolved presentation animation. Runtime state lives on the
/// projected UI entity; this asset definition remains immutable.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
pub struct UiNodeAnimationDefinition {
    pub duration_ms: u32,
    /// Reverse playback speed relative to forward playback.
    pub exit_rate: f32,
    pub initial_ms: u32,
    pub initial_forward: bool,
    /// Delay before forward playback begins.
    pub delay_ms: u32,
    /// Time held at the end before automatic reverse playback. Zero disables it.
    pub bob_ms: u32,
    pub interpolation: UiNodeAnimationInterpolation,
    pub start_rect: [f32; 4],
    pub end_rect: [f32; 4],
    /// Whether the authored animation supplies a color interval. This remains
    /// distinct from `affects_text_color`: an animation without a `colors`
    /// element leaves the node's authored visual tint unchanged.
    pub animates_color: bool,
    pub affects_text_color: bool,
    pub start_color: [u8; 4],
    pub end_color: [u8; 4],
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiNodeAnimationInterpolation {
    #[default]
    Linear,
    Sinusoidal,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiNodeVisualState {
    Default,
    Normal,
    Highlighted,
    Activated,
    Disabled,
    AlternateNormal,
    AlternateHighlighted,
    AlternateActivated,
    AlternateDisabled,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiNodeVisualStateDefinition {
    pub visual_state: UiNodeVisualState,
    pub image: AssetId,
    pub sound: AssetId,
    pub source_rect: UiNodeRegionDefinition,
    /// Texture modulation color authored directly on this visual state.
    pub color: [f32; 4],
    pub affects_color: bool,
    /// Text color authored by the visual state's font definition.
    pub text_color: [f32; 4],
    pub affects_text_color: bool,
    pub font: AssetId,
    pub hit_policy: UiNodePointerHitPolicy,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiNodePointerHitPolicy {
    Normal,
    Always,
    Never,
    Region,
}
