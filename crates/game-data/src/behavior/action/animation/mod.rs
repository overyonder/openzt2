use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorAnimationClipAction {
    pub animation_clip_asset_key: String,
    /// Repeat until the enclosing behavior cancels this action.
    #[serde(default)]
    pub looping: bool,
}
