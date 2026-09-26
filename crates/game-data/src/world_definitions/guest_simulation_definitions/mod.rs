//! Authored guest archetypes, arrivals, needs, memories, viewing, generation, surveys, and names.

use crate::AssetId;

mod guest_survey_signal_flag_operations;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum GuestNeedKind {
    Hunger,
    Thirst,
    Dessert,
    Gift,
    Energy,
    Restroom,
    Social,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum GuestVisitPurpose {
    View,
    Food,
    Drink,
    Restroom,
    Rest,
    Education,
    Shop,
    Exit,
    Show,
    Tour,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum GuestMemoryKind {
    AnimalView,
    Facility,
    Education,
    Crowding,
    Litter,
    Scenery,
    Danger,
    Show,
    Tour,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum MemoryReplacement {
    Oldest,
    LowestAbsoluteValue,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestMemoryPolicy {
    pub capacity: u16,
    pub retention_ticks: u64,
    pub replacement: MemoryReplacement,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct GuestDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub behavior_subject_type_identifiers: Vec<AssetId>,
    pub name_pool: AssetId,
    pub rarity: GuestRarity,
    pub preferred_animal_view_factor_q16: u32,
    pub starting_cash_cents: [i32; 2],
    pub initial_departure_points_q16: i32,
    pub radius_cm: u16,
    pub move_speed_mps: f32,
    pub patience_ticks: u32,
    pub needs: Vec<GuestNeedDefinition>,
    #[serde(default)]
    pub viewing_need: Option<GuestSignedNeedDefinition>,
    #[serde(default)]
    pub happiness_need: Option<GuestSignedNeedDefinition>,
    pub destination_weights: Vec<GuestDestinationWeight>,
    pub reactions: Vec<GuestReactionDefinition>,
    pub memory: GuestMemoryPolicy,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum GuestRarity {
    Common,
    Uncommon,
    Rare,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestNeedDefinition {
    pub kind: GuestNeedKind,
    pub initial_permille: [u16; 2],
    /// Wellness delta applied when the guest's authored need timer expires.
    pub adjustment_q16: i32,
    pub reconsider_threshold: u16,
    /// Authored deprivation permille at which an active need trigger clears.
    #[serde(default)]
    pub cessation_threshold: Option<u16>,
    pub critical_threshold: u16,
}
/// Authored viewing and happiness states use signed thresholds (cessation -100
/// in the official binder); these immutable policies retain source percentage units.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestSignedNeedDefinition {
    pub initial_q16: i32,
    pub adjustment_q16: i32,
    pub trigger_q16: Option<i32>,
    pub cessation_q16: Option<i32>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestDestinationWeight {
    pub purpose: GuestVisitPurpose,
    pub weight: i16,
    pub need: GuestNeedKind,
    pub minimum_need: u16,
    pub radius_cm: u32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestReactionDefinition {
    pub kind: GuestMemoryKind,
    pub satisfaction_delta: i16,
    pub education_delta: i16,
    pub retention_ticks: u64,
}

// Global authored limits for choosing a place from which a guest can see an
// animal. The original traversability manager owned the scan machinery; these
// are the immutable game rules left after Avian and Bevy own spatial queries
// and live relationships.
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct GuestViewingPolicy {
    pub ground_distance_m: f32,
    pub ground_range_m: f32,
    pub guest_eye_height_m: f32,
    pub minimum_open_cells: u16,
    pub maximum_slope_cos: f32,
}

// A constructed object's authored guest-standing locations.
// This schema record contains floating-point positions and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ViewingOpportunityDefinition {
    pub id: AssetId,
    pub priority: i16,
    pub slots: Vec<ViewingOpportunitySlot>,
}

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ViewingOpportunitySlot {
    pub local_position_m: [f32; 3],
}

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these independent authored guest-generation switches are not mutually exclusive states"
)]
pub struct GuestGenerationPolicy {
    pub need_adjustments_enabled: bool,
    /// Exclusive upper bound for random need delays, in 100 ms units.
    pub need_adjustment_delay_steps: u32,
    pub enabled: bool,
    pub use_test_type: bool,
    pub maximum_guests_base: u32,
    pub maximum_guests_per_half_star: u32,
    pub spawn_at_entrance: bool,
    pub arrival_offset_cm: [i32; 2],
    pub departure_offset_cm: [i32; 2],
    pub delay_ns: [u64; 2],
    pub default_emitter_cm: [i32; 2],
    pub stagger: bool,
    pub stagger_square_cm: u32,
    pub base_spawn_probability: f32,
    pub minimum_spawn_rate: f32,
    pub normal_admission_cents: [i32; 2],
    pub fame_rate_increase_permille_per_half_star: u16,
    pub rarity_thresholds: [u16; 2],
    pub price_adjustments: Vec<GuestPriceAdjustment>,
    pub species_adjustment: GuestSpeciesAdjustment,
    pub need_survey: GuestNeedSurveyPolicy,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct GuestPriceAdjustment {
    pub percent_range: [f32; 2],
    pub percent_delta: f32,
    pub percent_adjustment: f32,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct GuestSpeciesAdjustment {
    pub species_per_adjustment: u16,
    pub percent_per_adjustment: f32,
    pub maximum_percent_adjustment: f32,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestNeedSurveyPolicy {
    pub check_interval_ns: u64,
    pub maximum_critical_hits_per_month: u16,
    pub maximum_education_points_per_view: u16,
    pub maximum_entertainment_points_per_view: u16,
    pub monitored: GuestNeedSurveySignals,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct GuestNeedSurveySignals(u8);

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct PersonNamePool {
    pub id: AssetId,
    pub locale: AssetId,
    pub key: AssetId,
    pub delimiter: String,
    pub first_names: Vec<String>,
    pub last_names: Vec<String>,
}
