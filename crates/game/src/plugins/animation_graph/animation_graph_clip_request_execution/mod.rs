use bevy::prelude::*;
use openzt2_game_data::animation::animation_set::AuthoredAnimationGraphNode;

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackState,
    },
};

use super::{
    animation_graph_playback_message_types::{
        AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
    },
    animation_graph_playback_state_types::{
        AnimationGraphBlend, AnimationGraphEnabled, AnimationGraphPlaybackState,
    },
    animation_presentation_relationship_types::AnimationPresentationOwner,
    model_animation_asset_binding_types::ModelAnimationAssetLibrary,
};

pub(crate) fn execute_explicit_animation_clip_playback_requests(
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut animation_clip_requests: MessageReader<AnimationClipPlaybackRequest>,
    mut rejected_animation_clip_requests: MessageWriter<AnimationClipPlaybackRequestRejected>,
    mut animation_playback_controllers: Query<(
        Entity,
        &AnimationPresentationOwner,
        &ModelAnimationAssetLibrary,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&AnimationGraphEnabled>,
    )>,
) {
    for (animation_clip_request, request_id) in animation_clip_requests.read_with_id() {
        let mut started = false;
        for (
            animation_playback_controller_entity,
            animation_owner,
            animation_library,
            mut animation_playback_controller,
            mut animation_graph_playback_state,
            animation_graph_enabled,
        ) in &mut animation_playback_controllers
        {
            if animation_playback_controller_entity
                != animation_clip_request.animation_subject_entity
                && animation_owner.gameplay_entity
                    != animation_clip_request.animation_subject_entity
            {
                continue;
            }
            if animation_graph_enabled.is_some_and(|enabled| !enabled.0) {
                continue;
            }
            let Some((animation_set_asset_handle, animation_clip_index)) = animation_library
                .loaded_model_animation_set_asset_handles
                .iter()
                .find_map(|animation_set_asset_handle| {
                    let animation_set_asset =
                        animation_set_assets.get(animation_set_asset_handle)?;
                    animation_set_asset
                        .animation_clips
                        .iter()
                        .position(|animation_clip| {
                            animation_clip.animation_clip_asset_key
                                == animation_clip_request.animation_clip_asset_key
                        })
                        .and_then(|index| u32::try_from(index).ok())
                        .map(|index| (animation_set_asset_handle.clone(), index))
                })
            else {
                continue;
            };
            let Some(animation_set_asset) = animation_set_assets.get(&animation_set_asset_handle)
            else {
                continue;
            };
            let mut replacement_animation_graph_playback_state =
                AnimationGraphPlaybackState::default();
            replacement_animation_graph_playback_state.active_animation_graph_blend =
                (animation_clip_request.blend_duration_milliseconds != 0).then_some(
                    AnimationGraphBlend {
                        elapsed_milliseconds: 0,
                        duration_milliseconds: animation_clip_request.blend_duration_milliseconds,
                    },
                );
            if let Some(containing_graph_node_index) = animation_set_asset
                .animation_graph_nodes
                .iter()
                .position(|graph_node| {
                    animation_graph_node_contains_clip(
                        animation_set_asset,
                        graph_node,
                        animation_clip_index,
                    )
                })
                .and_then(|index| u32::try_from(index).ok())
            {
                replacement_animation_graph_playback_state.current_animation_graph_node_index =
                    containing_graph_node_index;
            }
            animation_playback_controller.animation_set_asset = animation_set_asset_handle;
            animation_playback_controller.playback_generation = animation_playback_controller
                .playback_generation
                .wrapping_add(1);
            animation_playback_controller.explicit_clip_request_id = Some(request_id.id);
            animation_playback_controller
                .animation_clip_asset_key
                .clone_from(&animation_clip_request.animation_clip_asset_key);
            animation_playback_controller.elapsed_milliseconds = 0;
            animation_playback_controller.playback_speed_permille =
                animation_clip_request.playback_speed_permille;
            animation_playback_controller.playback_state = AnimationPlaybackState::Playing;
            animation_playback_controller.playback_repetition_policy =
                animation_clip_request.playback_repetition_policy;
            *animation_graph_playback_state = replacement_animation_graph_playback_state;
            started = true;
            break;
        }
        if !started {
            warn!(subject = ?animation_clip_request.animation_subject_entity,
                clip = %animation_clip_request.animation_clip_asset_key,
                "explicit animation clip has no ready enabled presentation containing the clip");
            rejected_animation_clip_requests.write(AnimationClipPlaybackRequestRejected {
                request_id: request_id.id,
                animation_subject_entity: animation_clip_request.animation_subject_entity,
                animation_clip_asset_key: animation_clip_request.animation_clip_asset_key.clone(),
            });
        }
    }
}

fn animation_graph_node_contains_clip(
    animation_set_asset: &AnimationSetAsset,
    animation_graph_node: &AuthoredAnimationGraphNode,
    animation_clip_index: u32,
) -> bool {
    animation_set_asset
        .animation_clips
        .get(animation_clip_index as usize)
        .is_some_and(|animation_clip| {
            animation_graph_node
                .animation_clip_asset_keys
                .iter()
                .any(|member_asset_key| {
                    member_asset_key.eq_ignore_ascii_case(&animation_clip.animation_clip_asset_key)
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

    #[test]
    fn missing_presentation_rejects_the_exact_requested_clip() {
        let mut application = App::new();
        application
            .init_resource::<Assets<AnimationSetAsset>>()
            .add_message::<AnimationClipPlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequestRejected>()
            .add_systems(Update, execute_explicit_animation_clip_playback_requests);
        let subject = application.world_mut().spawn_empty().id();
        let request_id = application
            .world_mut()
            .write_message(AnimationClipPlaybackRequest {
                animation_subject_entity: subject,
                animation_clip_asset_key: "Stand_ShopGiveMoney".to_owned(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
            });
        application.update();
        let rejected: Vec<_> = application
            .world_mut()
            .resource_mut::<Messages<AnimationClipPlaybackRequestRejected>>()
            .drain()
            .collect();
        assert_eq!(rejected.len(), 1);
        assert_eq!(
            rejected[0].request_id,
            request_id.expect("registered clip request").id
        );
        assert_eq!(rejected[0].animation_subject_entity, subject);
        assert_eq!(rejected[0].animation_clip_asset_key, "Stand_ShopGiveMoney");
    }
}
