use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDockAction {
    pub subject_node_name: Option<String>,
    pub target_node_name: Option<String>,
    pub target_animation_clip_asset_key: Option<String>,
    pub locomotion_speed: Option<BehaviorDockLocomotionSpeed>,
    pub redock: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum BehaviorDockLocomotionSpeed {
    Slow,
    Medium,
    Fast,
}
