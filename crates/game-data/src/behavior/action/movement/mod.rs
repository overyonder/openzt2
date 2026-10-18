use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorMoveAction {
    /// Locomotion mode; absent uses the variable-speed default.
    pub locomotion_speed: Option<BehaviorMoveLocomotionSpeed>,
    pub target_node_name: Option<String>,
    /// Arrival tolerance in centimetres; the native default is 2.0 metres.
    pub move_radius_cm: u32,
}

/// Native `BFBehPursuit`: approach the task target, following it while it
/// moves, until within the hit radius.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorPursuitAction {
    pub locomotion_speed: Option<BehaviorMoveLocomotionSpeed>,
    /// Success distance from the target, in centimetres.
    pub hit_radius_cm: u32,
    /// The pursuit fails once the target is farther than this, in centimetres.
    pub failure_radius_cm: Option<u32>,
}

/// Named locomotion mode. Mode-specific speeds are not yet supported.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorMoveLocomotionSpeed {
    Slow,
    Medium,
    Fast,
}
