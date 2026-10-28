use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorMoveAction {
    /// Locomotion mode; absent uses the variable-speed default.
    pub locomotion_speed: Option<BehaviorMoveLocomotionSpeed>,
    pub target_node_name: Option<String>,
    /// Arrival tolerance in centimetres; the native default is 2.0 metres.
    pub move_radius_cm: u32,
}

/// Named gait selected from the actor’s authored locomotion table.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorMoveLocomotionSpeed {
    Slow,
    Medium,
    Fast,
}

/// Native `BFBehPursuit`: route toward the task target, then steer straight at
/// it once it is within the pursuit radius and the direct line is clear.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorPursuitAction {
    pub locomotion_speed: super::wander::BehaviorGroundLocomotionSpeed,
    /// Horizontal distance that completes the direct pursuit; native default 3 m.
    pub hit_radius_cm: u32,
    /// Distance inside which direct pursuit is tried; route legs end at half
    /// of it. Native default 12 m.
    pub pursuit_radius_cm: u32,
    /// Distance beyond which a poll fails the pursuit; native default 50 m.
    pub failure_radius_cm: u32,
    /// Seconds between failure and direct-line polls; native default 1 s.
    pub poll_interval_seconds: f32,
}
