use bevy::{gltf::Gltf, prelude::*};

use crate::{
    assets::animation::animation_set_asset_types::{AnimationClipRecord, AnimationSetAsset},
    plugins::{
        animation_playback::animation_playback_controller_types::AnimationPlaybackController,
        model_render::ModelExpanded, world_spawn::prefab_presentation_types::PrefabModel,
    },
};

use super::model_animation_asset_binding_types::{
    EmbeddedModelAnimationSetAssetHandlesByGltfAsset, ModelAnimationAttachmentResolved,
    PendingModelAnimationAssets,
};

pub(crate) fn resolve_loaded_gltf_animations_into_animation_set_assets(
    mut commands: Commands,
    loaded_gltf_assets: Res<Assets<Gltf>>,
    loaded_animation_clips: Res<Assets<AnimationClip>>,
    mut animation_set_assets: ResMut<Assets<AnimationSetAsset>>,
    mut embedded_model_animation_set_asset_handles_by_gltf_asset: ResMut<
        EmbeddedModelAnimationSetAssetHandlesByGltfAsset,
    >,
    unresolved_prefab_models: Query<
        (Entity, &PrefabModel),
        (
            With<ModelExpanded>,
            Without<AnimationPlaybackController>,
            Without<PendingModelAnimationAssets>,
            Without<ModelAnimationAttachmentResolved>,
        ),
    >,
) {
    for (prefab_model_entity, prefab_model) in &unresolved_prefab_models {
        let Some(model_handle) = prefab_model.handle.as_ref() else {
            continue;
        };
        let Some(loaded_gltf) = loaded_gltf_assets.get(model_handle) else {
            continue;
        };
        if loaded_gltf.animations.is_empty() {
            commands
                .entity(prefab_model_entity)
                .insert(ModelAnimationAttachmentResolved);
            continue;
        }
        if loaded_gltf.animations.iter().any(|animation_clip_handle| {
            loaded_animation_clips.get(animation_clip_handle).is_none()
        }) {
            continue;
        }
        let animation_set_asset_handle = embedded_model_animation_set_asset_handles_by_gltf_asset
            .get_or_create_animation_set_asset_handle_for_gltf_asset(model_handle.id(), || {
                create_animation_set_asset_from_loaded_gltf_embedded_animations(
                    loaded_gltf,
                    &loaded_animation_clips,
                    &mut animation_set_assets,
                )
            });
        commands
            .entity(prefab_model_entity)
            .insert(PendingModelAnimationAssets {
                pending_model_animation_set_asset_handles: vec![animation_set_asset_handle]
                    .into_boxed_slice(),
                requested_initial_animation_clip_asset_key: None,
            });
    }
}

fn create_animation_set_asset_from_loaded_gltf_embedded_animations(
    loaded_gltf_asset: &Gltf,
    loaded_animation_clip_assets: &Assets<AnimationClip>,
    animation_set_assets: &mut Assets<AnimationSetAsset>,
) -> Handle<AnimationSetAsset> {
    let embedded_animation_clip_records = loaded_gltf_asset
        .animations
        .iter()
        .enumerate()
        .map(|(animation_index, animation_clip_handle)| {
            AnimationClipRecord::create_embedded_animation_clip_record(
                loaded_gltf_asset
                    .named_animations
                    .iter()
                    .find_map(|(animation_name, candidate_animation_clip_handle)| {
                        (candidate_animation_clip_handle == animation_clip_handle)
                            .then(|| animation_name.to_string())
                    })
                    .unwrap_or_else(|| format!("animation_{animation_index}")),
                animation_clip_handle.clone(),
                (loaded_animation_clip_assets
                    .get(animation_clip_handle)
                    .expect("loaded glTF animation dependency disappeared")
                    .duration()
                    * 1000.0)
                    .max(1.0) as u32,
            )
        })
        .collect();
    animation_set_assets.add(AnimationSetAsset::create_ready_animation_set_asset(
        String::new(),
        String::new(),
        embedded_animation_clip_records,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    ))
}
