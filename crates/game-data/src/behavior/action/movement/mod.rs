use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorMoveAction {
    /// Locomotion mode; absent uses the variable-speed default.
    pub locomotion_speed: Option<BehaviorMoveLocomotionSpeed>,
    pub target_node_name: Option<String>,
    /// Arrival tolerance in centimetres; the native default is 2.0 metres.
    pub move_radius_cm: u32,
}

/// Named locomotion mode. Mode-specific speeds are not yet supported.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum BehaviorMoveLocomotionSpeed {
    Slow,
    Medium,
    Fast,
}
