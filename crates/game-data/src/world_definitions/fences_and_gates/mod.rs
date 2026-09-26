//! Authored fence segments, gates, and traversal blocking definitions.

use crate::AssetId;

mod fence_traversal_flag_operations;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct FenceTraversalBlockingFlags(u16);

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FenceDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub segment_length_cm: u16,
    pub height_cm: u16,
    pub strength: u16,
    pub blocks: FenceTraversalBlockingFlags,
    pub gate: AssetId,
    pub post_prefab: AssetId,
    pub segments: FenceSegmentPrefabs,
    pub gate_policy: FenceGatePolicy,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FenceSegmentPrefabs {
    pub cardinal_straight: AssetId,
    pub diagonal_straight: AssetId,
    pub cardinal_curve_90: AssetId,
    pub diagonal_curve_90: AssetId,
    pub cardinal_curve_135: AssetId,
    pub diagonal_curve_135: AssetId,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FenceGatePolicy {
    pub prefab: AssetId,
    pub open_animation: AssetId,
    pub close_animation: AssetId,
    pub trigger_distance_cm: u16,
    pub auto_close_ticks: u32,
}
