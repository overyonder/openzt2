use bevy::prelude::*;
use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationGraphEdge, AuthoredAnimationGraphNode,
};

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackRepetitionPolicy, AnimationPlaybackState,
    },
};

use super::{
    animation_graph_playback_message_types::AnimationGraphNodePlaybackRequestRejectionReason,
    animation_graph_playback_state_types::{
        AnimationGraphBlend, AnimationGraphPlaybackState, AnimationGraphTransition,
    },
    authored_animation_graph_edge_blend_duration_interpretation::interpret_authored_animation_graph_edge_blend_duration_milliseconds,
};

pub(super) fn begin_requested_animation_graph_node_transition(
    animation_set_asset: &AnimationSetAsset,
    target_animation_graph_node_index: u32,
    animation_playback_controller: &mut AnimationPlaybackController,
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_override_milliseconds: Option<u32>,
    enters_immediately: bool,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    if enters_immediately
        || target_animation_graph_node_index
            == animation_graph_playback_state.current_animation_graph_node_index
    {
        return enter_animation_graph_node_playback(
            animation_set_asset,
            target_animation_graph_node_index,
            animation_playback_controller,
            animation_graph_playback_state,
            blend_duration_override_milliseconds.unwrap_or_default(),
        );
    }
    let authored_animation_graph_edge = find_authored_animation_graph_edge_between_nodes(
        animation_set_asset,
        animation_graph_playback_state.current_animation_graph_node_index,
        target_animation_graph_node_index,
    )
    .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge)?;
    if animation_graph_playback_state
        .active_animation_graph_transition
        .is_some()
    {
        return Err(
            AnimationGraphNodePlaybackRequestRejectionReason::ActiveTransitionNotInterruptible,
        );
    }
    begin_authored_animation_graph_edge_transition(
        animation_set_asset,
        authored_animation_graph_edge,
        animation_playback_controller,
        animation_graph_playback_state,
        blend_duration_override_milliseconds,
    )
}

fn begin_authored_animation_graph_edge_transition(
    animation_set_asset: &AnimationSetAsset,
    authored_animation_graph_edge: &AuthoredAnimationGraphEdge,
    animation_playback_controller: &mut AnimationPlaybackController,
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_override_milliseconds: Option<u32>,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    let target_animation_graph_node_index = animation_set_asset
        .animation_graph_nodes
        .iter()
        .position(|animation_graph_node| {
            animation_graph_node.animation_graph_node_asset_key
                == authored_animation_graph_edge.target_animation_graph_node_asset_key
        })
        .and_then(|index| u32::try_from(index).ok())
        .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::UnknownAnimationGraphNode)?;
    let blend_duration_milliseconds = blend_duration_override_milliseconds.unwrap_or(u32::from(
        interpret_authored_animation_graph_edge_blend_duration_milliseconds(
            &authored_animation_graph_edge.authored_animation_attributes,
        ),
    ));
    if let Some(authored_transition_animation_clip) = authored_animation_graph_edge
        .transition_animation_clip_asset_keys
        .iter()
        .find_map(|animation_clip_asset_key| {
            animation_set_asset.find_animation_clip_record_by_asset_key(animation_clip_asset_key)
        })
    {
        begin_animation_graph_blend_timer(
            animation_graph_playback_state,
            blend_duration_milliseconds,
        );
        animation_playback_controller.playback_generation = animation_playback_controller
            .playback_generation
            .wrapping_add(1);
        animation_playback_controller.explicit_clip_request_id = None;
        animation_playback_controller
            .animation_clip_asset_key
            .clone_from(&authored_transition_animation_clip.animation_clip_asset_key);
        animation_playback_controller.elapsed_milliseconds = 0;
        animation_playback_controller.playback_state = AnimationPlaybackState::Playing;
        animation_playback_controller.playback_repetition_policy =
            AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy;
        animation_graph_playback_state.active_animation_graph_transition =
            Some(AnimationGraphTransition {
                target_animation_graph_node_index,
            });
        return Ok(());
    }
    enter_animation_graph_node_playback(
        animation_set_asset,
        target_animation_graph_node_index,
        animation_playback_controller,
        animation_graph_playback_state,
        blend_duration_milliseconds,
    )
}

pub(super) fn enter_animation_graph_node_playback(
    animation_set_asset: &AnimationSetAsset,
    animation_graph_node_index: u32,
    animation_playback_controller: &mut AnimationPlaybackController,
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_milliseconds: u32,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    let animation_graph_node = animation_set_asset
        .animation_graph_nodes
        .get(animation_graph_node_index as usize)
        .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::UnknownAnimationGraphNode)?;
    let animation_clip_asset_key =
        select_preferred_animation_clip_from_graph_node(animation_set_asset, animation_graph_node)
            .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAnimationAsset)?;
    begin_animation_graph_blend_timer(animation_graph_playback_state, blend_duration_milliseconds);
    animation_playback_controller.animation_clip_asset_key = animation_clip_asset_key;
    animation_playback_controller.playback_generation = animation_playback_controller
        .playback_generation
        .wrapping_add(1);
    animation_playback_controller.explicit_clip_request_id = None;
    animation_playback_controller.elapsed_milliseconds = 0;
    animation_playback_controller.playback_state = AnimationPlaybackState::Playing;
    animation_playback_controller.playback_repetition_policy =
        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy;
    animation_graph_playback_state.current_animation_graph_node_index = animation_graph_node_index;
    animation_graph_playback_state.active_animation_graph_transition = None;
    Ok(())
}

fn begin_animation_graph_blend_timer(
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_milliseconds: u32,
) {
    animation_graph_playback_state.active_animation_graph_blend =
        (blend_duration_milliseconds != 0).then_some(AnimationGraphBlend {
            elapsed_milliseconds: 0,
            duration_milliseconds: blend_duration_milliseconds,
        });
}

pub(crate) fn select_preferred_animation_clip_from_graph_node(
    animation_set_asset: &AnimationSetAsset,
    animation_graph_node: &AuthoredAnimationGraphNode,
) -> Option<String> {
    let animation_clips_in_graph_node = || {
        animation_graph_node
            .animation_clip_asset_keys
            .iter()
            .filter_map(|animation_clip_asset_key| {
                animation_set_asset
                    .find_animation_clip_record_by_asset_key(animation_clip_asset_key)
            })
    };
    animation_clips_in_graph_node()
        .find(|animation_clip| animation_clip.animation_clip_asset_key.ends_with("_Idle"))
        .or_else(|| {
            animation_clips_in_graph_node()
                .find(|animation_clip| animation_clip.animation_clip_asset_key.ends_with("_Ahead"))
        })
        .or_else(|| {
            animation_clips_in_graph_node().find(|animation_clip| {
                animation_clip.authored_animation_policy.explicit_use_only != Some(true)
            })
        })
        .or_else(|| animation_clips_in_graph_node().next())
        .map(|animation_clip| animation_clip.animation_clip_asset_key.clone())
}

fn find_authored_animation_graph_edge_between_nodes(
    animation_set_asset: &AnimationSetAsset,
    source_animation_graph_node_index: u32,
    target_animation_graph_node_index: u32,
) -> Option<&AuthoredAnimationGraphEdge> {
    let source_animation_graph_node_asset_key = animation_set_asset
        .animation_graph_nodes
        .get(source_animation_graph_node_index as usize)?
        .animation_graph_node_asset_key
        .as_str();
    let target_animation_graph_node_asset_key = animation_set_asset
        .animation_graph_nodes
        .get(target_animation_graph_node_index as usize)?
        .animation_graph_node_asset_key
        .as_str();
    animation_set_asset
        .animation_graph_edges
        .iter()
        .find(|authored_animation_graph_edge| {
            authored_animation_graph_edge.source_animation_graph_node_asset_key
                == source_animation_graph_node_asset_key
                && authored_animation_graph_edge.target_animation_graph_node_asset_key
                    == target_animation_graph_node_asset_key
        })
}
