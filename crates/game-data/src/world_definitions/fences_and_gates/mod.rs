//! Authored fence segments, gates, and traversal blocking definitions.

use crate::AssetId;

mod fence_traversal_flag_operations;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct FenceTraversalBlockingFlags(u16);

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
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
    #[serde(default)]
    pub fitting_curves: Vec<FenceFittingCurve>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct FenceFittingCurve {
    pub binder: AssetId,
    /// Local source coordinates, in authored segment order.
    pub points: Vec<[f32; 3]>,
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
#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FenceGatePolicy {
    /// Animated gate model, presented beside the edge's authored segment model.
    pub prefab: AssetId,
    /// Actor manifest owning the gate model's open and close clips.
    #[serde(default)]
    pub animation_set: AssetId,
    pub open_animation: String,
    pub close_animation: String,
    pub trigger_distance_cm: u16,
    pub auto_close_ticks: u32,
    #[serde(default)]
    pub close_distance_cm: u16,
    /// Simulation seconds, stored as nanoseconds; independent of zoo-calendar scaling.
    #[serde(default)]
    pub poll_interval_ns: u64,
    #[serde(default)]
    pub open_for_entity_type: Option<AssetId>,
    /// Types whose collision score becomes zero when this gate's `b_Open` is true.
    /// None retains unconditional passage for definitions without conditional scores.
    #[serde(default)]
    pub open_passage_types: Option<Vec<GateOpenCollisionException>>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GateOpenCollisionException {
    pub entity_type: AssetId,
    /// Authored ancestry categories used by the habitat boundary owner.
    pub traversal_types: FenceTraversalBlockingFlags,
}
