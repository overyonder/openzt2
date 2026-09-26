use bevy::prelude::*;

use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;

#[derive(Component, Clone, Debug)]
pub struct AnimationPlaybackController {
    /// Changes on every clip entry, including restarting the same clip.
    pub playback_generation: u64,
    /// Bevy message identity of the explicit clip request, when one owns playback.
    pub explicit_clip_request_id: Option<usize>,
    pub animation_set_asset: Handle<AnimationSetAsset>,
    pub animation_clip_asset_key: String,
    pub elapsed_milliseconds: u32,
    pub playback_speed_permille: i16,
    pub playback_state: AnimationPlaybackState,
    pub playback_repetition_policy: AnimationPlaybackRepetitionPolicy,
    pub playback_clock: AnimationPlaybackClock,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationPlaybackRepetitionPolicy {
    #[default]
    UseAuthoredClipPolicy,
    PlayOnce,
    Loop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationPlaybackState {
    #[default]
    Playing,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationPlaybackClock {
    #[default]
    Simulation,
    RealTime,
}
