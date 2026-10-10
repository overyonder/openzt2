use bevy::prelude::*;

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::{
        animation_playback::animation_playback_controller_types::{
            AnimationPlaybackClock, AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
            AnimationPlaybackState,
        },
        model_render::ModelExpanded,
        world_spawn::prefab_presentation_types::PrefabModel,
    },
};

use super::{
    animation_graph_playback_state_types::AnimationGraphPlaybackState,
    animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node,
    animation_presentation_relationship_types::AnimationPresentationOwner,
    model_animation_asset_binding_types::{
        ModelAnimationAssetLibrary, ModelAnimationAttachmentResolved, PendingModelAnimationAssets,
    },
};

pub(crate) fn attach_loaded_animation_set_assets_to_prefab_model_entities(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut animation_set_assets: ResMut<Assets<AnimationSetAsset>>,
    pending_prefab_models: Query<
        (
            Entity,
            &PrefabModel,
            Option<&ChildOf>,
            &PendingModelAnimationAssets,
        ),
        (
            With<ModelExpanded>,
            Without<AnimationPlaybackController>,
            Without<ModelAnimationAttachmentResolved>,
        ),
    >,
) {
    for (prefab_model_entity, prefab_model, prefab_model_parent, pending_model_animation_assets) in
        &pending_prefab_models
    {
        let pending_model_animation_set_asset_handles = pending_model_animation_assets
            .pending_model_animation_set_asset_handles
            .clone();
        let Some(_) = pending_model_animation_set_asset_handles.first() else {
            commands
                .entity(prefab_model_entity)
                .insert(ModelAnimationAttachmentResolved)
                .remove::<PendingModelAnimationAssets>();
            continue;
        };
        assert_eq!(
            pending_model_animation_set_asset_handles.len(),
            1,
            "a prefab model must reference exactly one model-specific animation set"
        );
        let initial_animation_set_asset_handle =
            pending_model_animation_set_asset_handles[0].clone();
        let loaded_model_animation_set_asset_handles =
            vec![initial_animation_set_asset_handle.clone()].into_boxed_slice();
        let Some(initial_animation_set_asset) =
            animation_set_assets.get(&initial_animation_set_asset_handle)
        else {
            continue;
        };
        assert!(
            initial_animation_set_asset.model_asset_path.is_empty()
                || openzt2_game_data::AssetId::from_virtual_path(
                    &initial_animation_set_asset.model_asset_path,
                ) == prefab_model.model
                // Archive paths are case-insensitive: manifests store them
                // lowercased while prefabs keep the authored casing.
                || initial_animation_set_asset
                    .model_asset_path
                    .eq_ignore_ascii_case(&prefab_model.model_path),
            "model-specific animation set {:?} does not belong to its prefab model {:?}",
            initial_animation_set_asset.model_asset_path,
            prefab_model.model_path,
        );
        let initial_animation_clip_asset_key = match pending_model_animation_assets
            .requested_initial_animation_clip_asset_key
            .clone()
            .map_or_else(
                || select_initial_animation_clip_asset_key(initial_animation_set_asset),
                Some,
            ) {
            Some(animation_clip_asset_key) => animation_clip_asset_key,
            None => {
                commands
                    .entity(prefab_model_entity)
                    .insert(ModelAnimationAttachmentResolved)
                    .remove::<PendingModelAnimationAssets>();
                continue;
            }
        };
        if initial_animation_set_asset
            .animation_clip_asset_load_has_failed(&initial_animation_clip_asset_key)
        {
            commands
                .entity(prefab_model_entity)
                .insert(ModelAnimationAttachmentResolved)
                .remove::<PendingModelAnimationAssets>();
            continue;
        }
        if initial_animation_set_asset
            .find_animation_clip_record_by_asset_key(&initial_animation_clip_asset_key)
            .is_none()
        {
            if !initial_animation_set_asset
                .animation_clip_asset_load_is_pending(&initial_animation_clip_asset_key)
            {
                let animation_clip_asset_load_was_requested = animation_set_assets
                    .get_mut(&initial_animation_set_asset_handle)
                    .expect("loaded animation asset disappeared")
                    .request_animation_clip_asset_load(
                        &asset_server,
                        &initial_animation_clip_asset_key,
                    );
                if !animation_clip_asset_load_was_requested {
                    commands
                        .entity(prefab_model_entity)
                        .insert(ModelAnimationAttachmentResolved)
                        .remove::<PendingModelAnimationAssets>();
                }
            }
            continue;
        }
        let initial_animation_set_asset = animation_set_assets
            .get(&initial_animation_set_asset_handle)
            .expect("loaded animation asset disappeared");
        let animation_clip_targets_are_external_to_the_model_scene =
            !initial_animation_set_asset.model_asset_path.is_empty();
        let initial_animation_graph_playback_state =
            (!initial_animation_set_asset.animation_graph_nodes.is_empty())
                .then(AnimationGraphPlaybackState::default);
        let mut prefab_model_commands = commands.entity(prefab_model_entity);
        prefab_model_commands.insert((
            ModelAnimationAttachmentResolved,
            AnimationPresentationOwner {
                gameplay_entity: prefab_model_parent.map_or(prefab_model_entity, ChildOf::parent),
            },
            ModelAnimationAssetLibrary {
                loaded_model_animation_set_asset_handles,
            },
            AnimationPlaybackController {
                playback_generation: 0,
                explicit_clip_request_id: None,
                animation_set_asset: initial_animation_set_asset_handle,
                animation_clip_asset_key: initial_animation_clip_asset_key,
                elapsed_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_state: AnimationPlaybackState::Playing,
                playback_repetition_policy:
                    AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
                playback_clock: AnimationPlaybackClock::Simulation,
            },
        ));
        if animation_clip_targets_are_external_to_the_model_scene {
            prefab_model_commands.insert(AnimationPlayer::default());
        }
        if let Some(animation_graph_playback_state) = initial_animation_graph_playback_state {
            prefab_model_commands.insert(animation_graph_playback_state);
        }
        prefab_model_commands.remove::<PendingModelAnimationAssets>();
    }
}

fn select_initial_animation_clip_asset_key(
    animation_set_asset: &AnimationSetAsset,
) -> Option<String> {
    animation_set_asset
        .animation_graph_order
        .first()
        .and_then(|graph_node_asset_key| {
            animation_set_asset
                .animation_graph_nodes
                .iter()
                .find(|animation_graph_node| {
                    &animation_graph_node.animation_graph_node_asset_key == graph_node_asset_key
                })
        })
        .or_else(|| animation_set_asset.animation_graph_nodes.first())
        .map_or_else(
            || {
                animation_set_asset
                    .animation_clips
                    .first()
                    .map(|animation_clip| animation_clip.animation_clip_asset_key.clone())
                    .or_else(|| {
                        animation_set_asset
                            .animation_clip_source_references
                            .first()
                            .map(|animation_clip_source_reference| {
                                animation_clip_source_reference
                                    .animation_clip_asset_key
                                    .clone()
                            })
                    })
            },
            |graph_node| {
                select_preferred_animation_clip_from_graph_node(animation_set_asset, graph_node)
                    .or_else(|| {
                        select_preferred_unloaded_animation_clip_from_graph_node(
                            animation_set_asset,
                            graph_node,
                        )
                    })
            },
        )
}

fn select_preferred_unloaded_animation_clip_from_graph_node(
    animation_set_asset: &AnimationSetAsset,
    animation_graph_node: &openzt2_game_data::animation::animation_set::AuthoredAnimationGraphNode,
) -> Option<String> {
    let animation_clip_source_references_in_graph_node = || {
        animation_graph_node
            .animation_clip_asset_keys
            .iter()
            .filter_map(|animation_clip_asset_key| {
                animation_set_asset
                    .find_animation_clip_source_reference_by_asset_key(animation_clip_asset_key)
            })
    };
    animation_clip_source_references_in_graph_node()
        .find(|animation_clip| animation_clip.animation_clip_asset_key.ends_with("_Idle"))
        .or_else(|| {
            animation_clip_source_references_in_graph_node()
                .find(|animation_clip| animation_clip.animation_clip_asset_key.ends_with("_Ahead"))
        })
        .or_else(|| {
            animation_clip_source_references_in_graph_node().find(|animation_clip| {
                animation_clip.playback_policy.explicit_use_only != Some(true)
            })
        })
        .or_else(|| animation_clip_source_references_in_graph_node().next())
        .map(|animation_clip| animation_clip.animation_clip_asset_key.clone())
}
