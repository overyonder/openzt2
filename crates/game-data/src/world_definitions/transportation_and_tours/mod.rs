//! Authored transport stations, tracks, vehicles, seats, and tour views.

use crate::AssetId;

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TransportationStationDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub kind: TransportationTrackKind,
    pub cardinal_endpoint_offsets_metres: Option<[[f32; 3]; 2]>,
    pub diagonal_endpoint_offsets_metres: Option<[[f32; 3]; 2]>,
    pub passenger_entry_offsets_cm: [[i16; 2]; 2],
    pub capacity: u16,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum TransportationTrackKind {
    Ground,
    Sky,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TransportationTrackDefinition {
    pub id: AssetId,
    pub kind: TransportationTrackKind,
    pub object: AssetId,
    pub cardinal_endpoint_offsets_metres: [[f32; 3]; 2],
    pub diagonal_endpoint_offsets_metres: [[f32; 3]; 2],
    pub max_grade_permille: u16,
    pub purchase_cost_cents: i64,
    #[serde(default)]
    pub sky_maximum_connection_distance_metres: Option<f32>,
    #[serde(default)]
    pub sky_rope_clearance_metres: Option<f32>,
    #[serde(default)]
    pub ground_piece_presentations: Vec<GroundTransportTrackPiecePresentationDefinition>,
    #[serde(default)]
    pub sky_tower_presentation: Option<SkyTowerTrackPresentationDefinition>,
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct GroundTransportTrackPiecePresentationDefinition {
    pub piece_type: u8,
    pub appearance: u8,
    pub ground_decal_texture: AssetId,
    pub ground_decal_rotation_radians: f32,
    pub ground_decal_size_metres: [f32; 2],
    pub ground_decal_offset_metres: [f32; 2],
    pub elevated_path_texture: AssetId,
    pub elevated_path_rotation_radians: f32,
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct SkyTowerTrackPresentationDefinition {
    /// Indexed by the connection kind: T = 0, L = 1, U-turn = 2.
    pub columns: [super::expanding_columns::ExpandingColumnPresentationDefinition; 3],
    pub initial_height_metres: f32,
    pub minimum_height_metres: f32,
    pub rope_texture: AssetId,
    pub rope_radius_metres: f32,
    pub rope_droop_metres: f32,
    pub rope_droop_maximum_length_metres: f32,
    pub rope_points_per_metre: f32,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TransportationVehicleDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub kind: TransportationTrackKind,
    pub purchase_cost_cents: i64,
    pub seats: u16,
    pub maximum_speed_metres_per_second: f32,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TransportationVehicleSeatDefinition {
    pub id: AssetId,
    pub vehicle: AssetId,
    pub index: u16,
    pub back_facing: bool,
    pub view_limits_degrees: [i16; 4],
    pub emote_bands: Vec<TransportationVehicleSeatEmoteBand>,
    pub animation_states: Vec<TransportationVehicleSeatAnimationState>,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TransportationVehicleSeatEmoteBand {
    pub behavior_set: AssetId,
    pub score: [f32; 2],
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TransportationVehicleSeatAnimationState {
    pub clip_prefix: String,
    pub angle_limits_degrees: [i16; 4],
    pub present_limits: u8,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TourViewDefinition {
    pub id: AssetId,
    pub subject: AssetId,
    pub radius_cm: u32,
    pub base_score: i16,
    pub dwell_ticks: u32,
    pub occlusion_required: bool,
}

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TourScoringPolicy {
    pub species_base_inducement: f32,
    pub view_event_curve: [f32; 2],
    pub static_object_curve: [f32; 2],
    pub animal_curve: [f32; 2],
    pub view_target_memory: u16,
    pub tour_history: u16,
    pub inducement_random_factor: [f32; 2],
    pub score_threshold: f32,
    pub need_threshold: f32,
    pub animal_feedback_probability: f32,
    pub animal_feedback_threshold: f32,
    pub object_feedback_thresholds: [f32; 2],
    pub categories: Vec<TourCategoryScore>,
    pub rating_ranges: Vec<TourRatingRange>,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TourCategoryScore {
    pub category: AssetId,
    pub value: i32,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct TourRatingRange {
    pub score: [f32; 2],
    pub rating: [f32; 2],
}
