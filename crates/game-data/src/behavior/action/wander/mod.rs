use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorGroundLocomotionSpeed {
    #[default]
    Slow,
    Medium,
    Fast,
    Glide,
    /// The locomotion row authored as `clean`, such as a keeper swimming
    /// along a tank while cleaning it.
    Clean,
}

/// Timed wandering with the actor's authored ground locomotion.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorWanderAction {
    pub minimum_duration: Duration,
    pub maximum_duration: Duration,
    pub avoid_water: bool,
    #[serde(default)]
    pub locomotion_speed: BehaviorGroundLocomotionSpeed,
    #[serde(default)]
    pub avoided_entities: Option<BehaviorWanderAvoidedEntities>,
    #[serde(default)]
    pub avoid_land: bool,
    /// Depth band below the water surface a swimming wanderer keeps to; both
    /// zero keeps it at the surface.
    #[serde(default)]
    pub minimum_depth_m: f32,
    #[serde(default)]
    pub maximum_depth_m: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorWanderAvoidedEntities {
    pub radius_m: f32,
    pub types: Vec<crate::AssetId>,
}

/// `BFBehEvasion`: move away from the task target and from entities of the
/// listed types until none is within `radius_m`, for at most
/// `maximum_duration`, when it ends in success unless `fail_on_timeout`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorEvasionAction {
    pub radius_m: f32,
    pub obstacle_radius_m: f32,
    pub maximum_duration: Duration,
    pub fail_on_timeout: bool,
    pub avoid_water: bool,
    pub avoid_land: bool,
    pub locomotion_speed: BehaviorGroundLocomotionSpeed,
    pub avoided_types: Vec<crate::AssetId>,
}
