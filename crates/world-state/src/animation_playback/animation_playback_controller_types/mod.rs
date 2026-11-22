use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};

#[derive(Component, Debug)]
pub struct RetainedAnimationClipSource(pub Handle<AnimationClipSourceAsset>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AnimationHeadLook {
    pub target: Entity,
    pub target_node: Option<String>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationGroundFitControls {
    pub quadruped_tilt_off: Option<bool>,
    pub prefer_quadruped: bool,
    pub leg_fit_enabled: Option<bool>,
}

impl Default for AnimationGroundFitControls {
    fn default() -> Self {
        Self {
            quadruped_tilt_off: None,
            prefer_quadruped: true,
            leg_fit_enabled: None,
        }
    }
}

impl AnimationGroundFitControls {
    pub const fn apply_authored_key(&mut self, mode: i32) {
        self.leg_fit_enabled = Some(mode != 0);
        match mode {
            0 => self.quadruped_tilt_off = Some(true),
            1 => self.quadruped_tilt_off = Some(false),
            2 => self.prefer_quadruped = false,
            4 => self.prefer_quadruped = true,
            _ => {}
        }
    }
}

#[derive(Component, Clone, Debug)]
#[require(AnimationGroundFitControls)]
pub struct AnimationPlaybackController {
    /// Changes on every clip entry, including restarting the same clip.
    pub playback_generation: u64,
    /// Bevy message identity of the explicit clip request, when one owns playback.
    pub explicit_clip_request_id: Option<usize>,
    pub animation_set_asset: Handle<AnimationSetAsset>,
    pub animation_clip_asset_key: String,
    pub preserve_seek_on_restart: bool,
    pub playback_speed_permille: i16,
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
pub enum AnimationPlaybackClock {
    #[default]
    Simulation,
    RealTime,
}
