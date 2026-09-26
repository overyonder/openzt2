//! Authored animal species, variants, needs, and compatibility.

mod species_flag_operations;

use serde::{Deserialize, Serialize};

use crate::AssetId;

/// Species facts contributed by one winning source document.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SpeciesDocument {
    pub species: Vec<Species>,
    pub variants: Vec<SpeciesVariantBinding>,
    pub compatibilities: Vec<SpeciesCompatibility>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpeciesVariantBinding {
    pub species: AssetId,
    pub variant: SpeciesVariant,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Species {
    pub id: AssetId,
    pub name_key: AssetId,
    pub taxonomy_key: AssetId,
    pub world_definition: AssetId,
    pub needs: Vec<SpeciesNeed>,
    pub adult_mass_kg: [f32; 2],
    pub stage_start_days: [f64; 4],
    pub lifespan_days: [f64; 2],
    pub gestation_days: f64,
    pub litter: [u16; 2],
    pub move_speed_mps: f32,
    pub swim_speed_mps: f32,
    pub appetite_per_day: f64,
    pub waste_definition: AssetId,
    pub waste_interval_days: f64,
    pub waste_units: u16,
    pub conservation: ConservationStatus,
    pub flags: SpeciesFlags,
    pub adoption_offer: AnimalAdoptionOfferDefinition,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnimalAdoptionOfferDefinition {
    pub rarity_fame_percent: u16,
    pub unlock_seconds_range: [f32; 2],
    pub remove_after_seconds: f32,
    pub dismiss_cooldown_seconds: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConservationStatus {
    #[default]
    Unspecified,
    LowRisk,
    Vulnerable,
    Endangered,
    Critical,
    Extinct,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SpeciesFlags(u32);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnimalNavigationCollisionPolicy {
    pub radius_m: f32,
    pub height_m: f32,
    pub depth_m: f32,
    pub default_triangle_score: i32,
    pub maximum_slope_radians: f32,
    pub escape_buffer_m: f32,
    pub wade_depth_m: f32,
    pub use_fast_pathing: bool,
    pub can_swim_without_entity: bool,
    pub can_swim_underwater: bool,
    pub can_use_water_freely: bool,
    pub water_score: i32,
    pub land_score: i32,
    pub motion_class: Option<AssetId>,
    pub required_type: Option<AssetId>,
    pub ignored_type: Option<AssetId>,
}

impl Default for AnimalNavigationCollisionPolicy {
    fn default() -> Self {
        Self {
            radius_m: 0.0,
            height_m: 0.0,
            depth_m: 0.0,
            default_triangle_score: 0,
            maximum_slope_radians: std::f32::consts::FRAC_PI_2,
            escape_buffer_m: 1.0,
            wade_depth_m: 0.0,
            use_fast_pathing: false,
            can_swim_without_entity: false,
            can_swim_underwater: false,
            can_use_water_freely: false,
            water_score: 100,
            land_score: 0,
            motion_class: None,
            required_type: None,
            ignored_type: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpeciesVariant {
    pub id: AssetId,
    /// Resolved authored type ancestry and enabled boolean classification facts.
    #[serde(default)]
    pub behavior_subject_type_identifiers: Vec<AssetId>,
    /// Resolved collision tester; absent when no tester is authored.
    #[serde(default)]
    pub navigation_collision_policy: Option<AnimalNavigationCollisionPolicy>,
    /// Source ground locomotion's `slow` clip (`variable` resolves to `slow`).
    #[serde(default)]
    pub ground_navigation_clip_asset_key: Option<String>,
    pub sex: Sex,
    pub life_stage: LifeStage,
    /// Canonical virtual path of the model asset.
    pub model: String,
    /// Canonical actor-manifest animation set for this model.
    pub model_animation_set: AssetId,
    /// Initial ground locomotion clip selected by the authored behavior set.
    pub initial_animation_clip_asset_key: Option<String>,
    /// Whether the authored initial ground locomotion clip repeats.
    pub initial_animation_loops: bool,
    /// Authored material variant key, when the source defines one.
    pub material_variant: Option<String>,
    pub scale: [f32; 2],
    pub weight_kg: [f32; 2],
    pub probability: u16,
    pub flags: SpeciesVariantFlags,
    pub adoption_count_range: [u16; 2],
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SpeciesVariantFlags(u8);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Sex {
    Female,
    Male,
    Any,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum LifeStage {
    Juvenile,
    Young,
    Adult,
    Elder,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum NeedKind {
    Hunger,
    Thirst,
    Rest,
    Privacy,
    Social,
    Exercise,
    Stimulation,
    Environment,
    Health,
    Hygiene,
    Bathroom,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeciesNeed {
    pub kind: NeedKind,
    pub initial_wellness_permille_range: [u16; 2],
    pub wellness_adjustment_per_update_q16: i32,
    pub cessation_wellness_threshold: Option<u16>,
    pub trigger_wellness_threshold: Option<u16>,
    pub critical_wellness_threshold: Option<u16>,
    pub display_wellness_threshold: Option<u16>,
    pub pressing_wellness_threshold: Option<u16>,
    pub advanced: bool,
    pub preferences: Vec<NeedPreference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NeedPreference {
    pub kind: NeedKind,
    pub target: AssetId,
    pub weight: i16,
    pub minimum_quality: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeciesCompatibility {
    pub species: AssetId,
    pub other: AssetId,
    pub relation: CompatibilityKind,
    pub score: i16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CompatibilityKind {
    Habitat,
    Social,
    Predator,
    Prey,
    Breeding,
}
