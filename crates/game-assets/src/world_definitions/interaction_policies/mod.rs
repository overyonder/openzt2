//! Authored inputs for object and guest interactions.

use openzt2_game_data::AssetId;
use std::time::Duration;

#[derive(Debug)]
pub struct StaffInteraction {
    pub kind: AssetId,
    pub property: Option<AssetId>,
    pub value: f32,
    pub threshold: f32,
    pub localization: Option<AssetId>,
    pub effect: Option<AssetId>,
    pub node: Option<AssetId>,
    pub delay: Duration,
    pub pre_script: Option<StaffInteractionScript>,
    pub post_script: Option<StaffInteractionScript>,
    pub show_selection: bool,
    pub use_parent_rotation: bool,
}

#[derive(Debug)]
pub struct StaffInteractionScript {
    pub context: AssetId,
    pub path: String,
    pub entry: String,
}

#[derive(Debug)]
pub struct GuestInfluenceWatch {
    pub category: AssetId,
    pub update_seconds: f64,
    pub feedback_chance_percent: f32,
    pub sensitivities: Vec<(AssetId, f32)>,
    pub ranges: Vec<GuestInfluenceRange>,
}

#[derive(Debug)]
pub struct GuestInfluenceRange {
    pub minimum: Option<f32>,
    pub maximum: Option<f32>,
    pub feedback: Vec<openzt2_game_data::behavior::action::feedback::BehaviorFeedbackEntry>,
    pub happiness_adjustment: f32,
    pub departure_adjustment: f32,
}

#[derive(Debug)]
pub struct InfluenceSource {
    pub category: AssetId,
    pub key: Option<AssetId>,
    pub strength: f32,
    pub radius_m: f32,
    pub radius_dropoff: AssetId,
    pub group_factor: f32,
    pub group_dropoff: AssetId,
    pub scaling_attribute: Option<AssetId>,
}

#[derive(Debug)]
pub struct GuestInfluencePolicy {
    pub cells_per_update: u32,
    pub update_delay_seconds: f32,
    pub native_factor: f32,
    pub biome_factor: f32,
    pub location_factor: f32,
    pub sources: Vec<InfluenceSource>,
}
