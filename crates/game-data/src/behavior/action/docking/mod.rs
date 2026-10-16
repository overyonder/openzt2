use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorWaterDockAction {
    pub subject_node_name: String,
    pub target_animation_clip_asset_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDockAction {
    pub subject_node_name: Option<String>,
    pub target_node_name: Option<String>,
    pub target_animation_clip_asset_key: Option<String>,
    pub locomotion_speed: Option<BehaviorDockLocomotionSpeed>,
    pub redock: bool,
    /// Radial docks stop this far from the target origin on the actor's side.
    pub radial_distance_m: Option<f32>,
    /// Distance within which the actor may finish approaching the dock point.
    pub arrival_radius_m: Option<f32>,
    /// Allowed heading difference from facing the target, in radians.
    pub rotation_tolerance_radians: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum BehaviorDockLocomotionSpeed {
    Slow,
    Medium,
    Fast,
}
