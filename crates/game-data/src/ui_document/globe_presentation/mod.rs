use crate::AssetId;
use serde::{Deserialize, Serialize};

/// Immutable globe models, placement, movement, and pointer presentation
/// lowered from the authored globe UI document.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiGlobePresentationDefinition {
    pub primary_model: AssetId,
    pub primary_translation: [f32; 3],
    pub clouds_model: AssetId,
    pub clouds_translation: [f32; 3],
    pub dot_model: AssetId,
    pub dot_translation: [f32; 3],
    pub selected_dot_model: AssetId,
    pub selected_dot_translation: [f32; 3],
    pub pointer_model: AssetId,
    pub selection_rotate_speed: f32,
    pub mouse_increment: f32,
    pub mouse_down_friction: f32,
    pub mouse_up_friction: f32,
    pub friction_transition_seconds: f32,
    pub move_seconds: f32,
    pub scream_threshold: f32,
    pub scream_delay_seconds: f32,
    pub dot_highlight_rgba: [u8; 4],
    pub dot_highlight_cursor: AssetId,
    pub biome_models: Vec<UiGlobeBiomeModelDefinition>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiGlobeBiomeModelDefinition {
    pub biome: AssetId,
    pub model: AssetId,
}
