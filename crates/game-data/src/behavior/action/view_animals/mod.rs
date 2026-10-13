//! Authored animal viewing retained by the guest viewing action.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorViewAnimalsAction {
    /// The view meter's authored start and the range it is drawn from.
    pub view_meter_start: f32,
    pub view_meter_start_range: [f32; 2],
    /// Behavior set that moves the viewer to a viewpoint before watching.
    pub dock_viewpoint_behavior_set: Option<crate::AssetId>,
    /// Whether the viewer may pick another animal while watching.
    pub retarget: Option<bool>,
    /// Emote sets played while the view meter lies in their range.
    pub emote_sets: Vec<ViewAnimalsEmoteSet>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ViewAnimalsEmoteSet {
    pub behavior_set: crate::AssetId,
    pub view_meter_range: [f32; 2],
}
