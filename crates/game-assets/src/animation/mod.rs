//! Animation-set and animation-clip asset loading and source lowering.

pub mod animation_set_asset_types;

pub mod animation_clip_source_asset_loading;
pub mod animation_clip_source_queries;
pub mod animation_set_source_asset_loading;
pub mod bfm_animation_set_lowering;
pub mod kfm_animation_set_lowering;

use bevy::{asset::AssetApp, prelude::*};

use animation_clip_source_asset_loading::AnimationClipSourceAssetLoader;
use animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset};
use animation_set_source_asset_loading::AnimationSetSourceAssetLoader;

#[derive(Default)]
pub struct AnimationAssetLoadingPlugin;

impl Plugin for AnimationAssetLoadingPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<AnimationSetAsset>()
            .init_asset::<AnimationClipSourceAsset>()
            .init_asset_loader::<AnimationSetSourceAssetLoader>()
            .init_asset_loader::<AnimationClipSourceAssetLoader>();
    }
}
