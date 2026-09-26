//! Animation-set and animation-clip asset loading and source lowering.

pub mod animation_set_asset_types;

mod animation_clip_source_asset_loading;
mod animation_set_clip_hydration;
mod animation_set_source_asset_loading;
mod bfm_animation_set_lowering;
mod kfm_animation_set_lowering;

use bevy::{asset::AssetApp, prelude::*};

use animation_clip_source_asset_loading::AnimationClipSourceAssetLoader;
use animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset};
use animation_set_clip_hydration::hydrate_animation_sets_after_requested_clip_assets_load;
use animation_set_source_asset_loading::AnimationSetSourceAssetLoader;

#[derive(Default)]
pub struct AnimationAssetLoadingPlugin;

impl Plugin for AnimationAssetLoadingPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<AnimationSetAsset>()
            .init_asset::<AnimationClipSourceAsset>()
            .init_asset_loader::<AnimationSetSourceAssetLoader>()
            .preregister_asset_loader::<AnimationClipSourceAssetLoader>(&["bf", "kf"])
            .add_systems(
                PreUpdate,
                hydrate_animation_sets_after_requested_clip_assets_load,
            );
    }

    fn finish(&self, application: &mut App) {
        let animation_clip_source_loader =
            AnimationClipSourceAssetLoader::from_world(application.world_mut());
        application.register_asset_loader(animation_clip_source_loader);
    }
}
