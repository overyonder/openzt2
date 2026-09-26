use serde::{Deserialize, Serialize};

use crate::AssetId;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorTargetTestAction {
    pub target_test_kind: BehaviorTargetTestKind,
    pub target_definition_asset_id: Option<AssetId>,
    pub success_behavior_set_asset_id: Option<AssetId>,
    pub failure_behavior_set_asset_id: Option<AssetId>,
    pub target_radius_cm: u32,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorTargetTestKind {
    Fence,
    Position,
}
