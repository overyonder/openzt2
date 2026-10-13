use bevy::gltf::Gltf;
use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

#[derive(Resource, Default)]
pub struct EmbeddedModelAnimationSetAssetHandlesByGltfAsset {
    animation_set_asset_handles_by_gltf_asset:
        bevy::platform::collections::HashMap<AssetId<Gltf>, Handle<AnimationSetAsset>>,
}

impl EmbeddedModelAnimationSetAssetHandlesByGltfAsset {
    pub fn get_or_create_animation_set_asset_handle_for_gltf_asset(
        &mut self,
        gltf_asset_id: AssetId<Gltf>,
        create_animation_set_asset_handle: impl FnOnce() -> Handle<AnimationSetAsset>,
    ) -> Handle<AnimationSetAsset> {
        self.animation_set_asset_handles_by_gltf_asset
            .entry(gltf_asset_id)
            .or_insert_with(create_animation_set_asset_handle)
            .clone()
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ModelAnimationAttachmentResolved;

#[derive(Component, Clone, Debug)]
pub struct PendingModelAnimationAssets {
    pub pending_model_animation_set_asset_handles: Box<[Handle<AnimationSetAsset>]>,
    pub requested_initial_animation_clip_asset_key: Option<String>,
}

/// Complete authored animation library retained by one model instance.
#[derive(Component, Clone, Debug)]
pub struct ModelAnimationAssetLibrary {
    pub loaded_model_animation_set_asset_handles: Box<[Handle<AnimationSetAsset>]>,
}
