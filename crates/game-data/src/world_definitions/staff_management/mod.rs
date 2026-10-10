//! Authored staff roles, jobs, capabilities, and task policies.

use crate::AssetId;

mod staff_job_flag_operations;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum StaffRoleKind {
    None,
    Keeper,
    Maintenance,
    Educator,
    Veterinarian,
    Entertainer,
    Trainer,
    Presenter,
    Paleontologist,
    Recovery,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum StaffJobKind {
    Feed,
    RefillWater,
    CleanHabitat,
    EmptyBin,
    SweepLitter,
    Repair,
    Treat,
    Educate,
    Entertain,
    Tranquilize,
    Capture,
    MaintainTank,
    OperateShow,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct StaffJobCapabilityFlags(u32);

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffRoleDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub name_pool: AssetId,
    pub role: StaffRoleKind,
    pub model_animation_set: AssetId,
    pub initial_animation_clip_asset_key: Option<String>,
    pub initial_animation_loops: bool,
    pub wage_cents_per_month: i32,
    pub move_speed_mps: f32,
    pub navigation_radius_m: f32,
    pub permitted_jobs: StaffJobCapabilityFlags,
    pub job_overrides: Vec<AssetId>,
    /// Concrete authored looks (sex, skin and uniform textures, head) a hired
    /// employee is drawn from. Empty roles present the role object's prefab.
    #[serde(default)]
    pub presentation_variants: Vec<StaffPresentationVariant>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffPresentationVariant {
    pub prefab: AssetId,
    /// Actor manifest animations authored for this look's body.
    pub model_animation_set: AssetId,
    pub name_pool: AssetId,
    pub head: Option<StaffHeadPresentation>,
    pub texture_replacement_sets: Vec<super::world_objects::WorldObjectTextureReplacementSet>,
}

/// Separate head model the original attaches to a body joint.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub struct StaffHeadPresentation {
    pub prefab: AssetId,
    pub joint: String,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub enum StaffJobEffect {
    Fill { amount: u16 },
    Clean { amount: u16 },
    Repair { amount: u16 },
    Treat { treatment: AssetId },
    Educate { amount: u16 },
    Tranquilize { definition: AssetId },
    Capture,
    Operate,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffJobDefinition {
    pub id: AssetId,
    pub kind: StaffJobKind,
    pub duration_ticks: u32,
    pub interaction_radius_cm: u16,
    pub capability: StaffJobCapabilityFlags,
    pub effect: StaffJobEffect,
    pub priority: i16,
}

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffAquaticTankWaterCleaningPolicy {
    pub token: AssetId,
    pub value: f32,
    pub localization_key: AssetId,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[repr(u8)]
pub enum StaffRequestThresholdComparison {
    /// Mode 0: predicate is `current == threshold`; fires on entering equality.
    EntersEquality = 0,
    /// Mode 1: predicate is `current < threshold`; fresh on downward crossing,
    /// strict on the current value and inclusive on the previous value.
    FallsBelow = 1,
    /// Mode 2: predicate is `current > threshold`; fresh on upward crossing,
    /// strict on the current value and inclusive on the previous value.
    RisesAbove = 2,
    /// Mode 3: predicate is `current <= threshold`; fresh on downward crossing,
    /// inclusive on the current value.
    ReachesOrBelow = 3,
    /// Mode 4: predicate is `current >= threshold`; fresh on upward crossing,
    /// inclusive on the current value.
    ReachesOrAbove = 4,
}

impl StaffRequestThresholdComparison {
    /// Rejects authored mode numbers outside the supported 0..=4 set instead of
    /// guessing a comparison for them.
    #[must_use]
    pub const fn from_authored_test_type(test_type: i32) -> Option<Self> {
        match test_type {
            0 => Some(Self::EntersEquality),
            1 => Some(Self::FallsBelow),
            2 => Some(Self::RisesAbove),
            3 => Some(Self::ReachesOrBelow),
            4 => Some(Self::ReachesOrAbove),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub enum StaffRequestThresholdValue {
    Boolean(bool),
    Number(f32),
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "authored priority is a float"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffRequestData {
    pub token: Option<AssetId>,
    pub subject_type: Option<AssetId>,
    pub priority: f32,
    pub target: Option<AssetId>,
    pub staff: Option<AssetId>,
}

impl StaffRequestData {
    /// Default request: priority 2.0 and no token, subject, target, or staff.
    pub const CONSTRUCTED_DEFAULT: Self = Self {
        token: None,
        subject_type: None,
        priority: 2.0,
        target: None,
        staff: None,
    };
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "thresholds contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffRequestControllerDefinition {
    /// Entity family that owns the binder (`id(record.key)`).
    pub id: AssetId,
    /// Authored `BFNamedBinder` binder name; `None` for a plain `BFBinder`.
    pub binder: Option<AssetId>,
    /// Authored `attribName` tracked attribute key, for example `f_FoodLevel`,
    /// `hunger`, or `b_Escaped`. Controllers without an attribute key only use
    /// the creation and destruction trigger flags.
    pub attribute_key: Option<AssetId>,
    pub threshold: StaffRequestThresholdValue,
    /// Constructed default `ReachesOrAbove` (testType 4).
    pub threshold_comparison: StaffRequestThresholdComparison,
    pub cancel_threshold: StaffRequestThresholdValue,
    /// Constructed default `FallsBelow` (cancelTestType 1).
    pub cancel_comparison: StaffRequestThresholdComparison,
    /// Constructed default `false`.
    pub trigger_on_creation: bool,
    /// Defaults to `false`; currently unused.
    pub trigger_on_destruction: bool,
    pub request: StaffRequestData,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub struct StaffTokenDispatchRule {
    /// `StaffTokens` child element name, for example `t_FillFoodContainer`.
    pub token: AssetId,
    /// Nested `BFAIToken Name`; native XML keeps this identity separately from
    /// the parent entry name, so lowering preserves both without assuming they
    /// must match.
    pub name: Option<AssetId>,
    pub give_to: Option<AssetId>,
    pub payload: Option<AssetId>,
    pub force: Option<bool>,
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "policy contains floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct StaffManagerPolicy {
    pub id: AssetId,
    pub tokens: Vec<StaffTokenDispatchRule>,
    pub care_types: Vec<AssetId>,
    /// Constructed default 6.0.
    pub job_safe_distance_m: f32,
    pub steal_job_threshold: f32,
    /// Constructed default 30.0; no shipped staffMgr entry sets it.
    pub bad_entity_cleanup_interval: f32,
    /// Native fossil-site sensor scan delay; constructed/config fallback 120 seconds.
    pub paleontologist_search_delay_seconds: f32,
    /// Constructed default 1; expansion staffMgr entries author 300.
    pub time_category_duration: i32,
    /// Authored `DisplacementX`/`DisplacementY`; consumers are not implemented.
    pub displacement_m: [f32; 2],
    pub default_request: StaffRequestData,
}
