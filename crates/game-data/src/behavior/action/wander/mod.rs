use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorGroundLocomotionSpeed {
    #[default]
    Slow,
    Medium,
    Fast,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorWanderAvoidedEntities {
    pub radius_m: f32,
    pub types: Vec<crate::AssetId>,
}
