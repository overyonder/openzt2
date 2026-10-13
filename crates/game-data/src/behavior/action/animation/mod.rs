use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorAnimationClipAction {
    pub animation_clip_asset_key: String,
    /// Repeat until the enclosing behavior cancels this action.
    #[serde(default)]
    pub looping: bool,
    /// The named physical object that performs the clip, such as a gift cart's
    /// `Vendor`. Its animations belong to its owner, so the owner's clip request
    /// reaches the one presentation that has the clip.
    #[serde(default)]
    pub physical_object: Option<String>,
    /// `playTime`: the clip repeats for this many seconds, then the action
    /// ends, as when an aardvark sleeps curled for 20 seconds.
    #[serde(default)]
    pub play_duration_seconds: Option<f32>,
    /// `interruptFlag`: the clip replaces the current animation at once,
    /// rather than waiting to reach it through the animation graph.
    #[serde(default)]
    pub interrupts_current_animation: bool,
    /// `interruptible`: a newly chosen task may cut the clip short.
    #[serde(default)]
    pub interruptible: bool,
    /// `groundFit`: whether the actor follows the terrain slope while the clip
    /// plays. Absent keeps the actor's own setting.
    #[serde(default)]
    pub ground_fit: Option<bool>,
    /// `blendDuration`: seconds to blend from the previous animation.
    #[serde(default)]
    pub blend_duration_seconds: Option<f32>,
    /// `animSpeed`, or a `minAnimSpeed`..`maxAnimSpeed` range sampled each
    /// time the clip starts. One is normal speed.
    #[serde(default)]
    pub playback_speed: Option<BehaviorAnimationPlaybackSpeed>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum BehaviorAnimationPlaybackSpeed {
    Fixed(f32),
    Range { minimum: f32, maximum: f32 },
}
