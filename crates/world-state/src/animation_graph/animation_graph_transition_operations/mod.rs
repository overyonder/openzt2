use bevy::prelude::*;
use openzt2_game_data::animation::animation_set::AuthoredAnimationGraphEdge;
use openzt2_game_data::animation::animation_set::AuthoredAnimationGraphNode;

use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackState;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use crate::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequestRejectionReason;
use crate::animation_graph::animation_graph_playback_state_types::AnimationGraphBlend;
use crate::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use crate::animation_graph::animation_graph_playback_state_types::AnimationGraphTransition;
use crate::animation_graph::authored_animation_graph_edge_blend_duration_interpretation::interpret_authored_animation_graph_edge_blend_duration_milliseconds;

pub fn begin_requested_animation_graph_node_transition(
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
    if animation_graph_playback_state
        .active_animation_graph_transition
        .is_some()
    {
        return Err(
            AnimationGraphNodePlaybackRequestRejectionReason::ActiveTransitionNotInterruptible,
        );
    }
    let mut route = find_animation_graph_transition_route(
        animation_set_asset,
        animation_graph_playback_state.current_animation_graph_node_index,
        target_animation_graph_node_index,
    )
    .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge)?;
    // A missing transition clip must not turn an authored edge into a teleport.
    let mut source = animation_graph_playback_state.current_animation_graph_node_index;
    for target in &route {
        let edge =
            find_authored_animation_graph_edge_between_nodes(animation_set_asset, source, *target)
                .ok_or(
                    AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge,
                )?;
        if (!edge.transition_animation_clip_asset_keys.is_empty()
            && !edge
                .transition_animation_clip_asset_keys
                .iter()
                .any(|clip| {
                    animation_set_asset
                        .find_animation_clip_record_by_asset_key(clip)
                        .is_some()
                }))
            || select_preferred_animation_clip_from_graph_node(
                animation_set_asset,
                &animation_set_asset.animation_graph_nodes[*target as usize],
            )
            .is_none()
        {
            return Err(AnimationGraphNodePlaybackRequestRejectionReason::MissingAnimationAsset);
        }
        source = *target;
    }
    let next = route
        .pop_front()
        .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge)?;
    let authored_animation_graph_edge = find_authored_animation_graph_edge_between_nodes(
        animation_set_asset,
        animation_graph_playback_state.current_animation_graph_node_index,
        next,
    )
    .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge)?;
    begin_authored_animation_graph_edge_transition(
        animation_set_asset,
        authored_animation_graph_edge,
        animation_playback_controller,
        animation_graph_playback_state,
        blend_duration_override_milliseconds,
    )?;
    animation_graph_playback_state.remaining_transition_nodes = route;
    animation_graph_playback_state.route_owner = None;
    continue_animation_graph_transition_route(
        animation_set_asset,
        animation_playback_controller,
        animation_graph_playback_state,
    )
}

/// Shortest authored directed route, excluding the starting node.
pub fn find_animation_graph_transition_route(
    asset: &AnimationSetAsset,
    source: u32,
    target: u32,
) -> Option<std::collections::VecDeque<u32>> {
    let count = asset.animation_graph_nodes.len();
    if source as usize >= count || target as usize >= count {
        return None;
    }
    let mut predecessors = vec![None; count];
    let mut queue = std::collections::VecDeque::from([source]);
    predecessors[source as usize] = Some(source);
    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }
        for edge in &asset.animation_graph_edges {
            if edge.source_animation_graph_node_asset_key
                != asset.animation_graph_nodes[current as usize].animation_graph_node_asset_key
            {
                continue;
            }
            let Some(next) = asset
                .animation_graph_nodes
                .iter()
                .position(|node| {
                    node.animation_graph_node_asset_key
                        == edge.target_animation_graph_node_asset_key
                })
                .and_then(|index| u32::try_from(index).ok())
            else {
                continue;
            };
            if predecessors[next as usize].is_none() {
                predecessors[next as usize] = Some(current);
                queue.push_back(next);
            }
        }
    }
    let mut route = std::collections::VecDeque::new();
    let mut current = target;
    while current != source {
        route.push_front(current);
        current = predecessors[current as usize]?;
    }
    Some(route)
}

pub fn continue_animation_graph_transition_route(
    asset: &AnimationSetAsset,
    controller: &mut AnimationPlaybackController,
    state: &mut AnimationGraphPlaybackState,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    while state.active_animation_graph_transition.is_none() {
        let Some(next) = state.remaining_transition_nodes.pop_front() else {
            break;
        };
        let edge = find_authored_animation_graph_edge_between_nodes(
            asset,
            state.current_animation_graph_node_index,
            next,
        )
        .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge)?;
        let remaining = std::mem::take(&mut state.remaining_transition_nodes);
        begin_authored_animation_graph_edge_transition(asset, edge, controller, state, None)?;
        state.remaining_transition_nodes = remaining;
    }
    Ok(())
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
        animation_playback_controller
            .fractional_milliseconds
            .clear();
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

pub fn enter_animation_graph_node_playback(
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
    animation_playback_controller
        .fractional_milliseconds
        .clear();
    animation_playback_controller.playback_state = AnimationPlaybackState::Playing;
    animation_playback_controller.playback_repetition_policy =
        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy;
    animation_graph_playback_state.current_animation_graph_node_index = animation_graph_node_index;
    animation_graph_playback_state.active_animation_graph_transition = None;
    animation_graph_playback_state
        .remaining_transition_nodes
        .clear();
    animation_graph_playback_state.route_owner = None;
    Ok(())
}

fn begin_animation_graph_blend_timer(
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_milliseconds: u32,
) {
    animation_graph_playback_state.active_animation_graph_blend =
        (blend_duration_milliseconds != 0).then_some(AnimationGraphBlend {
            fractional_milliseconds: default(),
            elapsed_milliseconds: 0,
            duration_milliseconds: blend_duration_milliseconds,
        });
}

pub fn select_preferred_animation_clip_from_graph_node(
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
