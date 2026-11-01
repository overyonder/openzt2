use serde::{Deserialize, Serialize};

use crate::AssetId;

/// `BFBehLocoSwitchSet`: play the behavior set authored for the subject's
/// locomotion medium when the action starts.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorLocomotionSwitchSetAction {
    pub ground: Option<AssetId>,
    pub water: Option<AssetId>,
    pub underwater: Option<AssetId>,
    /// Without `switch="false"` the set follows the subject's medium while it
    /// plays, switching to the other medium's set when that medium changes.
    #[serde(default)]
    pub follows_medium_changes: bool,
}

/// `BFBehAnimSwitchSet`: play the behavior set authored for the animation
/// graph node the subject is in, such as `Stand` or `Lie`, when it starts.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BehaviorAnimationSwitchSetAction {
    /// Graph node names with the set each selects, in authored order.
    pub sets_by_animation_graph_node: Vec<(String, AssetId)>,
    /// The `default` row, for any other node.
    pub default_set: Option<AssetId>,
    /// `loopFlag="true"` selects again each time the selected set ends.
    #[serde(default)]
    pub loops: bool,
}
