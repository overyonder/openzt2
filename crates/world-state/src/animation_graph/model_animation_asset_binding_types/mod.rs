use bevy::prelude::*;

use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ModelAnimationAttachmentResolved;

#[derive(Component, Clone, Debug)]
pub struct PendingModelAnimationAssets {
    pub pending_model_animation_set_asset_handles: Box<[Handle<AnimationSetAsset>]>,
    pub requested_initial_animation_clip_asset_key: Option<String>,
    pub requested_initial_repetition_policy: Option<AnimationPlaybackRepetitionPolicy>,
}

/// Complete authored animation library retained by one model instance.
#[derive(Component, Clone, Debug)]
pub struct ModelAnimationAssetLibrary {
    pub loaded_model_animation_set_asset_handles: Box<[Handle<AnimationSetAsset>]>,
}
