use std::sync::Arc;

use bevy::{asset::LoadState, prelude::*};

use super::animation_set_asset_types::{
    AnimationClipRecord, AnimationClipSourceAsset, AnimationSetAsset,
};

pub(super) fn hydrate_animation_sets_after_requested_clip_assets_load(
    asset_server: Res<AssetServer>,
    animation_clip_source_assets: Res<Assets<AnimationClipSourceAsset>>,
    mut animation_set_assets: ResMut<Assets<AnimationSetAsset>>,
) {
    let mut loaded_animation_clip_records = Vec::new();
    let mut failed_animation_clip_asset_keys = Vec::new();
    for (animation_set_asset_id, animation_set_asset) in animation_set_assets.iter() {
        for (animation_clip_asset_key, animation_clip_source_asset_handle) in
            &animation_set_asset.pending_animation_clip_source_asset_handles
        {
            if let Some(loaded_animation_clip_source_asset) =
                animation_clip_source_assets.get(animation_clip_source_asset_handle)
            {
                let authored_animation_policy = animation_set_asset
                    .find_animation_clip_source_reference_by_asset_key(animation_clip_asset_key)
                    .map(|animation_clip_source_reference| {
                        animation_clip_source_reference.playback_policy.clone()
                    })
                    .unwrap_or_default();
                loaded_animation_clip_records.push((
                    animation_set_asset_id,
                    AnimationClipRecord {
                        animation_clip_asset_key: animation_clip_asset_key.clone(),
                        animation_clip_handle: loaded_animation_clip_source_asset
                            .animation_clip_handle
                            .clone(),
                        duration_milliseconds: loaded_animation_clip_source_asset
                            .authored_animation_clip_metadata
                            .duration_milliseconds
                            .max(1),
                        authored_animation_clip_metadata: Some(Arc::clone(
                            &loaded_animation_clip_source_asset.authored_animation_clip_metadata,
                        )),
                        authored_animation_policy,
                    },
                ));
            } else if matches!(
                asset_server.load_state(animation_clip_source_asset_handle.id()),
                LoadState::Failed(_)
            ) {
                failed_animation_clip_asset_keys
                    .push((animation_set_asset_id, animation_clip_asset_key.clone()));
            }
        }
    }
    for (animation_set_asset_id, loaded_animation_clip_record) in loaded_animation_clip_records {
        let Some(mut animation_set_asset) = animation_set_assets.get_mut(animation_set_asset_id)
        else {
            continue;
        };
        animation_set_asset
            .pending_animation_clip_source_asset_handles
            .retain(|(animation_clip_asset_key, _)| {
                animation_clip_asset_key != &loaded_animation_clip_record.animation_clip_asset_key
            });
        animation_set_asset
            .animation_clips
            .push(loaded_animation_clip_record);
    }
    for (animation_set_asset_id, failed_animation_clip_asset_key) in
        failed_animation_clip_asset_keys
    {
        let Some(mut animation_set_asset) = animation_set_assets.get_mut(animation_set_asset_id)
        else {
            continue;
        };
        animation_set_asset
            .pending_animation_clip_source_asset_handles
            .retain(|(animation_clip_asset_key, _)| {
                animation_clip_asset_key != &failed_animation_clip_asset_key
            });
        animation_set_asset
            .failed_animation_clip_asset_keys
            .push(failed_animation_clip_asset_key);
    }
}
