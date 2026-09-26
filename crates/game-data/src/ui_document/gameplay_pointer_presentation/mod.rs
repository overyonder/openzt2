use crate::AssetId;
use serde::{Deserialize, Serialize};

/// Selected-layer pointer cursors authored by the original gameplay mode tree.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiGameplayInteractionCursorDefinition {
    pub overhead: AssetId,
    pub placement_default: AssetId,
    pub placement_pickup: AssetId,
    pub placement_rotate: AssetId,
    pub fence_default: AssetId,
    pub fence_gate: AssetId,
    pub path: AssetId,
    pub elevated_path: AssetId,
    pub biome_default: AssetId,
    pub biome_paint: AssetId,
    pub biome_invalid: AssetId,
    pub terrain: AssetId,
    pub tank: AssetId,
    pub selection_default: AssetId,
    pub selection_pickup: AssetId,
    pub selection_rotate: AssetId,
    pub delete: AssetId,
}

/// Construction preview textures, colors, and grid sizes authored beside the
/// gameplay cursor definitions in the original mode tree.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct UiConstructionPlacementPreviewDefinition {
    pub footprint_valid: AssetId,
    pub footprint_valid_small: AssetId,
    pub footprint_invalid: AssetId,
    pub footprint_invalid_small: AssetId,
    pub grid_large_cardinal: AssetId,
    pub grid_large_diagonal: AssetId,
    pub grid_large_radius: f32,
    pub grid_medium_cardinal: AssetId,
    pub grid_medium_diagonal: AssetId,
    pub grid_medium_radius: f32,
    pub grid_small_cardinal: AssetId,
    pub grid_small_diagonal: AssetId,
    pub grid_small_radius: f32,
    pub model_valid_srgba: [u8; 4],
    pub model_invalid_srgba: [u8; 4],
    pub fence_valid_srgba: [u8; 4],
    pub fence_invalid_srgba: [u8; 4],
    pub footprint_valid_srgba: [u8; 4],
    pub footprint_invalid_srgba: [u8; 4],
    pub grid_srgba: [u8; 4],
}
