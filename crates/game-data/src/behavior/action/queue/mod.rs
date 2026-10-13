//! Authored queue approach and waiting policy; live membership belongs to ECS.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorQueueWaitAction {
    /// Optional container override; otherwise use the enclosing task's tag.
    pub container: Option<crate::AssetId>,
    /// Behavior repeated while capacity is unavailable.
    pub waiting_behavior_set: crate::AssetId,
    /// Lower bound of the one-time waiting deadline draw.
    pub minimum_wait: std::time::Duration,
    /// Upper bound of the one-time waiting deadline draw.
    pub maximum_wait: std::time::Duration,
}

/// `BFBehDockQueue`: walk up to the target's queue for the enclosing task's
/// container without taking a service place.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDockQueueAction {
    /// The actor stops this far from the queue's docking node.
    pub hit_radius_m: f32,
    /// The authored reach of the waiting line. Line formation is not modelled.
    pub queue_radius_m: f32,
    pub locomotion_speed: Option<super::docking::BehaviorDockLocomotionSpeed>,
}
