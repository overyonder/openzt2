//! Authored animal-show stages, upgrades, tricks, and show rules.

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowStageDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub performer_slots: u16,
    pub audience_capacity: u16,
    pub schedule_slots: u16,
    pub supported_tricks: Vec<AssetId>,
    pub admission_cents: i32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowPlatformUpgradePolicy {
    pub canopy_cost_cents: i64,
    pub canopy_resale_cents: i64,
    pub canopy_definition: AssetId,
    pub canopy_attachment: AssetId,
    pub television_cost_cents: i64,
    pub television_resale_cents: i64,
    pub television_definition: AssetId,
    pub television_attachment: AssetId,
}
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TrickDefinition {
    pub id: AssetId,
    pub species: Vec<AssetId>,
    pub animation: AssetId,
    pub training_ticks: u32,
    pub difficulty: u16,
    pub welfare_cost: u16,
    pub entertainment: u16,
    pub name_key: AssetId,
    pub icon: AssetId,
    pub popularity_q16: i32,
    pub display_order: u32,
    pub target: AssetId,
    pub prerequisite: Option<TrickPrerequisite>,
    pub levels: Vec<TrickLevel>,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TrickPrerequisite {
    pub trick: AssetId,
    pub minimum_score: u16,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TrickLevel {
    pub minimum_score: u16,
    pub failure_percent: u8,
    pub success_percent: u8,
    pub critical_percent: u8,
    pub gesture: Option<AssetId>,
    pub learning_delta: u16,
    pub staff_can_train: bool,
    pub can_perform: bool,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct TrickOutcomeTokens {
    pub id: AssetId,
    pub failure: AssetId,
    pub success: AssetId,
    pub critical: AssetId,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowRuleDefinition {
    pub id: AssetId,
    pub minimum_tricks: u16,
    pub duration_ticks: [u32; 2],
    pub cooldown_ticks: u32,
    pub payout_per_guest_cents: i32,
}

// Immutable show-facing prefab, behavior, welfare, and feedback references.
// Focused show presentation systems project these onto the participating stage
// and master-of-ceremonies entities.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowPresentationPolicy {
    pub feedback: [AssetId; 2],
    pub behavior_tokens: [AssetId; 6],
    pub master_of_ceremonies_spawn_node: AssetId,
    pub master_of_ceremonies_type: AssetId,
    pub master_of_ceremonies_tokens: [AssetId; 5],
    pub master_of_ceremonies_fade_ns: u64,
    pub master_of_ceremonies_replace_timeout_ns: u64,
    pub need_adjustment_category: AssetId,
}
// Timing and admission limits consumed by each stage's `ShowState`; this is
// policy only and never owns a live schedule or countdown.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowSchedulingPolicy {
    pub time_category_ns: u64,
    pub donation_token_timeout_ns: u64,
    pub summon_dismiss_timeout_ns: u64,
    pub trick_treat_timeout_ns: u64,
    pub between_show_ns: u64,
    pub pre_show_ns: u64,
    pub post_show_ns: u64,
    pub maximum_admitting_ns: u64,
    pub maximum_travel_cm_per_second: u32,
    pub timeslots_per_show: u16,
    pub presentation_update_interval_ns: u64,
    pub editor_update_interval_ns: u64,
}
// Show result thresholds consumed directly by the focused scoring system.
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ShowScoringPolicy {
    pub minimum_score: f32,
    pub percent_tricks_to_score: f32,
    pub score_update_factor: f32,
    pub score_bands: Vec<ShowScoreBand>,
    pub size_bands: Vec<ShowSizeBand>,
}
// Cue-to-sound join. Shows emit the selected sound through a typed request;
// Bevy Audio owns its handle and playback entity.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowAudioCue {
    pub id: AssetId,
    pub sound: AssetId,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ShowScoreBand {
    pub minimum_tricks: u16,
    pub minimum_score: f32,
    pub show_score: f32,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ShowSizeBand {
    pub minimum_show_score: f32,
    pub guest_count: [u16; 2],
}
// Static layout/style facts consumed when Bevy UI projects the show editor.
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowEditorPresentationPolicy {
    pub show_score_pips_dimension: u16,
    pub show_score_scaled_width: u16,
    pub animal_score_pips_dimension: u16,
    pub animal_score_scaled_width: u16,
    pub droplist_z_adjustment: i16,
    pub maximum_animals: u16,
    pub trick_dropdown_x_adjustment: i16,
    pub highlight_rgb: [u8; 3],
    pub icons: Vec<AssetId>,
}
// Show-editor icon lookup consumed as a texture handle reference.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ShowPresentationIcon {
    pub id: AssetId,
    pub texture: AssetId,
}
