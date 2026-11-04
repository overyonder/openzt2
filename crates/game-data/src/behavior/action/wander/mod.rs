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
    Stop,
    /// Any other row of the actor's locomotion table, such as `sprint` or `fly`.
    Named(crate::AssetId),
}

impl BehaviorGroundLocomotionSpeed {
    #[must_use]
    pub fn from_authored(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "slow" => Self::Slow,
            "medium" => Self::Medium,
            "fast" => Self::Fast,
            "glide" => Self::Glide,
            "clean" => Self::Clean,
            "stop" => Self::Stop,
            other => Self::Named(crate::AssetId::from_key(other)),
        }
    }

    /// The key of this speed's row in a locomotion table.
    #[must_use]
    pub fn row_key(self) -> crate::AssetId {
        crate::AssetId::from_key(match self {
            Self::Slow => "slow",
            Self::Medium => "medium",
            Self::Fast => "fast",
            Self::Glide => "glide",
            Self::Clean => "clean",
            Self::Stop => "stop",
            Self::Named(key) => return key,
        })
    }
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
