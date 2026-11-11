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

/// The shipped `ZTDiseaseMgr`: when the zoo exposes animals to each authored
/// disease. Durations are seconds of game time.
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct DiseaseExposurePolicy {
    pub check_interval_seconds: f32,
    /// Kept while its role in the manager is unknown.
    pub update_interval_seconds: f32,
    /// Kept while its role in the manager is unknown.
    pub cure_timeout_seconds: f32,
    /// Kept while its role in the manager is unknown.
    pub half_star_minimum: u8,
    pub diseases: Vec<AuthoredDisease>,
}

/// One `ZTDisease`. Scripts and tasks run its stages; the manager only gives
/// a qualifying animal its `token`.
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct AuthoredDisease {
    pub name: AssetId,
    pub token: AssetId,
    pub cure_token: AssetId,
    pub clear_tokens: Vec<AssetId>,
    pub target_types: Vec<AssetId>,
    pub chance_percent: f32,
    /// A type whose zoo count scales `chance_percent`, such as `TrashGround`.
    pub chance_multiplier: Option<AssetId>,
    /// Kept while its role in the manager is unknown.
    pub single_qualifier: bool,
    /// `false` when the disease is only given through its cause object's tasks.
    pub search_for_targets: bool,
    /// Only a script gives this disease, as for `TutorialLove`.
    pub force_only: bool,
    pub zoo_tests: Vec<DiseaseZooTest>,
    pub animal_filters: Vec<DiseaseAnimalFilter>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum DiseaseComparison {
    Less,
    LessOrEqual,
    Equal,
    GreaterOrEqual,
    Greater,
}

impl DiseaseComparison {
    #[must_use]
    pub fn holds(self, value: u32, reference: u32) -> bool {
        match self {
            Self::Less => value < reference,
            Self::LessOrEqual => value <= reference,
            Self::Equal => value == reference,
            Self::GreaterOrEqual => value >= reference,
            Self::Greater => value > reference,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum DiseaseZooTest {
    /// `ZTEntityTypePredicate`: how many entities of a type, optionally with
    /// a boolean attribute, the zoo holds.
    EntityCount {
        entity_type: AssetId,
        attribute: Option<(AssetId, bool)>,
        quantity: u32,
        comparison: DiseaseComparison,
        /// Kept while its role in the predicate is unknown.
        different: bool,
    },
    /// `ZTZooFamePredicate`.
    Fame {
        half_stars: u32,
        comparison: DiseaseComparison,
    },
}

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub enum DiseaseAnimalFilter {
    /// `ZTAnimalAttributeFilter`.
    Attribute { attribute: AssetId, value: bool },
    /// `ZTAnimalProximityFilter`: an entity of the type within (`closer`) or
    /// beyond the distance.
    Proximity {
        entity_type: AssetId,
        distance_m: f32,
        closer: bool,
    },
}
