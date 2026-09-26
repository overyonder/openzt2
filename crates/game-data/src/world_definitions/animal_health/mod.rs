//! Authored disease, treatment, tranquilizer, and rampage definitions.

use super::staff_management::StaffRoleKind;
use crate::AssetId;

mod animal_health_flag_operations;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct DiseaseDefinition {
    pub id: AssetId,
    pub eligible_species: Vec<AssetId>,
    pub check_interval_ticks: u32,
    pub chance_per_check: u32,
    pub severity_per_tick_q16: i32,
    pub vitality_per_tick_q16: i32,
    pub symptoms: Vec<AssetId>,
    pub treatments: Vec<AssetId>,
    pub fatal_threshold: u16,
    pub hint_thresholds: [u16; 3],
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TreatmentDefinition {
    pub id: AssetId,
    pub disease: AssetId,
    pub required_staff: StaffRoleKind,
    pub duration_ticks: u32,
    pub severity_delta: i16,
    pub vitality_delta: i16,
    pub research: AssetId,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TranquilizerEligibility(u8);

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TranquilizerDefinition {
    pub id: AssetId,
    pub duration_ticks: u32,
    pub eligible_states: TranquilizerEligibility,
    pub recovery_ticks: u32,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TranquilizerModePolicy {
    pub range_cm: u32,
    pub charge_points_per_second: f32,
    pub charge_decrease_points_per_second: f32,
    pub required_charge_points: f32,
    pub use_distance_cm: u32,
    pub shot_cue: AssetId,
    pub misfire_cue: AssetId,
    pub begin_charge_cue: AssetId,
    pub charge_cue: AssetId,
    pub end_charge_cue: AssetId,
    pub lose_tracking_cue: AssetId,
    pub shot_effect: AssetId,
    pub shot_effect_distance_cm: i32,
    pub shot_effect_offset_cm: [i32; 2],
    pub recoil_duration_seconds: f32,
    pub misfire_duration_seconds: f32,
    pub reticle_charging: AssetId,
    pub reticle_ready: AssetId,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct RampageRule {
    pub id: AssetId,
    pub species: AssetId,
    pub welfare_below: u16,
    pub disease_above: u16,
    pub probability: u32,
    pub minimum_ticks: u32,
    pub behavior: AssetId,
}
