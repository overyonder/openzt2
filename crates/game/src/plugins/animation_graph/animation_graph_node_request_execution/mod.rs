use bevy::prelude::*;

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackController,
};

use super::animation_graph_playback_message_types::{
    AnimationGraphNodePlaybackRequest, AnimationGraphNodePlaybackRequestRejected,
    AnimationGraphNodePlaybackRequestRejectionReason,
};
use super::animation_graph_playback_state_types::{
    AnimationGraphEnabled, AnimationGraphPlaybackState,
};
use super::animation_graph_transition_operations::{
    begin_requested_animation_graph_node_transition, enter_animation_graph_node_playback,
};
use super::animation_presentation_relationship_types::AnimationPresentationOwner;
use super::model_animation_asset_binding_types::ModelAnimationAssetLibrary;

pub(crate) fn execute_animation_graph_node_playback_requests(
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut animation_graph_node_playback_requests: MessageReader<AnimationGraphNodePlaybackRequest>,
    mut rejected_animation_graph_node_playback_requests: MessageWriter<
        AnimationGraphNodePlaybackRequestRejected,
    >,
    mut animation_playback_controllers: Query<(
        Entity,
        &AnimationPresentationOwner,
        &ModelAnimationAssetLibrary,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&AnimationGraphEnabled>,
    )>,
) {
    for animation_graph_node_playback_request in animation_graph_node_playback_requests.read() {
        let mut matching_animation_playback_controller_was_found = false;
        let mut rejection_reason = None;
        for (
            animation_playback_controller_entity,
            animation_presentation_owner,
            animation_library,
            mut animation_playback_controller,
            mut animation_graph_playback_state,
            animation_graph_enabled,
        ) in &mut animation_playback_controllers
        {
            if animation_playback_controller_entity
                != animation_graph_node_playback_request.animation_subject_entity
                && animation_presentation_owner.gameplay_entity
                    != animation_graph_node_playback_request.animation_subject_entity
            {
                continue;
            }
            matching_animation_playback_controller_was_found = true;
            if animation_graph_enabled
                .is_some_and(|animation_graph_enabled| !animation_graph_enabled.0)
            {
                rejection_reason =
                    Some(AnimationGraphNodePlaybackRequestRejectionReason::AnimationGraphDisabled);
                continue;
            }
            let Some(current_animation_set_asset) =
                animation_set_assets.get(&animation_playback_controller.animation_set_asset)
            else {
                rejection_reason =
                    Some(AnimationGraphNodePlaybackRequestRejectionReason::MissingAnimationAsset);
                continue;
            };
            let requested_current_graph_node_index = current_animation_set_asset
                .animation_graph_nodes
                .iter()
                .position(|animation_graph_node| {
                    animation_graph_node.animation_graph_node_asset_key
                        == animation_graph_node_playback_request.animation_graph_node_asset_key
                })
                .and_then(|index| u32::try_from(index).ok());
            if let Some(requested_animation_graph_node_index) = requested_current_graph_node_index {
                let previous_elapsed_milliseconds =
                    animation_playback_controller.elapsed_milliseconds;
                match begin_requested_animation_graph_node_transition(
                    current_animation_set_asset,
                    requested_animation_graph_node_index,
                    &mut animation_playback_controller,
                    &mut animation_graph_playback_state,
                    animation_graph_node_playback_request.blend_duration_milliseconds,
                    animation_graph_node_playback_request.enters_immediately,
                ) {
                    Err(animation_graph_node_request_rejection_reason) => {
                        rejection_reason = Some(animation_graph_node_request_rejection_reason);
                    }
                    Ok(()) => {
                        if animation_graph_node_playback_request.preserves_current_playback_time {
                            animation_playback_controller.elapsed_milliseconds =
                                previous_elapsed_milliseconds;
                        }
                    }
                }
                continue;
            }
            let requested_alternate_graph_node = animation_library
                .loaded_model_animation_set_asset_handles
                .iter()
                .find_map(|animation_set_asset_handle| {
                    let animation_set_asset =
                        animation_set_assets.get(animation_set_asset_handle)?;
                    animation_set_asset
                        .animation_graph_nodes
                        .iter()
                        .position(|animation_graph_node| {
                            animation_graph_node.animation_graph_node_asset_key
                                == animation_graph_node_playback_request
                                    .animation_graph_node_asset_key
                        })
                        .and_then(|index| u32::try_from(index).ok())
                        .map(|animation_graph_node_index| {
                            (
                                animation_set_asset_handle.clone(),
                                animation_graph_node_index,
                            )
                        })
                });
            let Some((animation_set_asset_handle, requested_animation_graph_node_index)) =
                requested_alternate_graph_node
            else {
                rejection_reason = Some(
                    AnimationGraphNodePlaybackRequestRejectionReason::UnknownAnimationGraphNode,
                );
                continue;
            };
            let requested_animation_set_asset = animation_set_assets
                .get(&animation_set_asset_handle)
                .expect("target graph was resolved from the loaded animation asset library");
            let mut replacement_animation_graph_playback_state =
                AnimationGraphPlaybackState::default();
            let previous_elapsed_milliseconds = animation_playback_controller.elapsed_milliseconds;
            match enter_animation_graph_node_playback(
                requested_animation_set_asset,
                requested_animation_graph_node_index,
                &mut animation_playback_controller,
                &mut replacement_animation_graph_playback_state,
                animation_graph_node_playback_request
                    .blend_duration_milliseconds
                    .unwrap_or_default(),
            ) {
                Err(animation_graph_node_request_rejection_reason) => {
                    rejection_reason = Some(animation_graph_node_request_rejection_reason);
                }
                Ok(()) => {
                    if animation_graph_node_playback_request.preserves_current_playback_time {
                        animation_playback_controller.elapsed_milliseconds =
                            previous_elapsed_milliseconds;
                    }
                    animation_playback_controller.animation_set_asset = animation_set_asset_handle;
                    *animation_graph_playback_state = replacement_animation_graph_playback_state;
                }
            }
        }
        if rejection_reason.is_some() {
            reject_animation_graph_node_playback_request(
                &mut rejected_animation_graph_node_playback_requests,
                animation_graph_node_playback_request.animation_subject_entity,
            );
        } else if !matching_animation_playback_controller_was_found {
            reject_animation_graph_node_playback_request(
                &mut rejected_animation_graph_node_playback_requests,
                animation_graph_node_playback_request.animation_subject_entity,
            );
        }
    }
}

fn reject_animation_graph_node_playback_request(
    rejected_animation_graph_node_playback_requests: &mut MessageWriter<
        AnimationGraphNodePlaybackRequestRejected,
    >,
    animation_subject_entity: Entity,
) {
    rejected_animation_graph_node_playback_requests.write(
        AnimationGraphNodePlaybackRequestRejected {
            animation_subject_entity,
        },
    );
}
