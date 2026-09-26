//! Authored fossil recovery, assembly, and cloning definitions.

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FossilSetDefinition {
    pub id: AssetId,
    pub species: AssetId,
    pub pieces: Vec<AssetId>,
    pub completion_unlock: AssetId,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FossilPieceDefinition {
    pub id: AssetId,
    pub set: AssetId,
    pub model: AssetId,
    pub slot: AssetId,
    pub discovery_weight: u16,
    pub scale_permille: u16,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct FossilSlotDefinition {
    pub id: AssetId,
    pub transform: [[f32; 4]; 4],
    pub tolerance_cm: u16,
    pub tolerance_degrees: u16,
}
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct FossilPlacementPolicy {
    pub puzzle_root: AssetId,
    pub entity_root: AssetId,
    pub placeable_objects: Vec<AssetId>,
    pub non_placeable_objects: Vec<AssetId>,
    pub minimum_sonar_distance_squared: f32,
    pub maximum_sonar_distance_squared: f32,
    pub minimum_sonar_view_dot: f32,
    /// Radius and forward offset of the authored fossil dig area.
    pub dig_distance_m: f32,
}
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct CloningCenterDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub difficulty_levels: Vec<CloningDifficultyLevel>,
    pub failure_outcomes: Vec<CloningFailureOutcome>,
    pub sickly_threshold: u16,
    pub normal_threshold: u16,
    pub super_threshold: u16,
}
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct CloningDifficultyLevel {
    pub level: u8,
    pub gesture_count: u8,
    pub research_duration_ns: u64,
    pub trace_speed: f32,
    pub trace_speed_increment: f32,
    pub trace_speed_counter: u16,
    pub gestures: Vec<AssetId>,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct CloningFailureOutcome {
    pub definition: AssetId,
    pub test_type: AssetId,
    pub weight: u16,
    /// Zero means the authored outcome is unlimited.
    pub limit: u16,
}
