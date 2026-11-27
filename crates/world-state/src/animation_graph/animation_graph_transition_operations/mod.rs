use bevy::prelude::*;
use openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationClipMetadata;
use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationGraphEdge, AuthoredAnimationGraphNode, AuthoredKfmTransition,
    KfmTransitionKind,
};

use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use crate::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequestRejectionReason;
use crate::animation_graph::animation_graph_playback_state_types::AnimationGraphTransition;
use crate::animation_graph::animation_graph_playback_state_types::{
    AnimationGraphChainCursor, AnimationGraphPlaybackState, AnimationGraphRoute,
    AnimationGraphRouteStep,
};
use crate::animation_graph::authored_animation_graph_edge_blend_duration_interpretation::interpret_authored_animation_graph_edge_blend_duration_milliseconds;

/// Shortest directed route through the asset's ordered outgoing edge index.
pub fn resolve_animation_graph_route<'a>(
    node_count: usize,
    outgoing_edges: impl Fn(u32) -> &'a [(u32, usize)],
    source: u32,
    target: u32,
) -> Option<AnimationGraphRoute> {
    if source as usize >= node_count || target as usize >= node_count {
        return None;
    }
    if source == target {
        return Some(AnimationGraphRoute {
            source_node: source,
            target_node: target,
            steps: Default::default(),
        });
    }
    let mut entered_by = vec![None; node_count];
    let mut queue = std::collections::VecDeque::from([source]);
    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }
        for &(next, edge_index) in outgoing_edges(current) {
            let Some(entry) = entered_by.get_mut(next as usize) else {
                continue;
            };
            if next != source && entry.is_none() {
                *entry = Some(AnimationGraphRouteStep {
                    source_node: current,
                    target_node: next,
                    edge_index,
                });
                queue.push_back(next);
            }
        }
    }
    let mut steps = std::collections::VecDeque::new();
    let mut current = target;
    while current != source {
        let step = entered_by[current as usize]?;
        steps.push_front(step);
        current = step.source_node;
    }
    Some(AnimationGraphRoute {
        source_node: source,
        target_node: target,
        steps,
    })
}

pub fn begin_resolved_animation_graph_route(
    asset: &AnimationSetAsset,
    clip_ready: impl Fn(&str) -> bool + Copy,
    mut route: AnimationGraphRoute,
    controller: &mut AnimationPlaybackController,
    state: &mut AnimationGraphPlaybackState,
    blend_override_ms: Option<u32>,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    use AnimationGraphNodePlaybackRequestRejectionReason::{
        ActiveTransitionNotInterruptible, MissingAnimationAsset, MissingAuthoredGraphEdge,
    };
    if route.source_node != state.current_animation_graph_node_index {
        return Err(MissingAuthoredGraphEdge);
    }
    if route.target_node == route.source_node {
        return enter_animation_graph_node_playback(
            asset,
            clip_ready,
            route.target_node,
            controller,
            state,
            blend_override_ms.unwrap_or_default(),
        );
    }
    if state.active_animation_graph_transition.is_some() {
        return Err(ActiveTransitionNotInterruptible);
    }
    let mut source = route.source_node;
    for &step in &route.steps {
        let (edge, target) =
            resolve_route_step(asset, source, step).ok_or(MissingAuthoredGraphEdge)?;
        if !transition_clips_ready(edge, clip_ready)
            || !target
                .animation_clip_asset_keys
                .iter()
                .any(|clip| clip_ready(clip))
        {
            return Err(MissingAnimationAsset);
        }
        source = step.target_node;
    }
    if source != route.target_node {
        return Err(MissingAuthoredGraphEdge);
    }
    let first = route.steps.pop_front().ok_or(MissingAuthoredGraphEdge)?;
    begin_authored_animation_graph_edge_transition(
        asset,
        clip_ready,
        first,
        controller,
        state,
        blend_override_ms,
    )?;
    state.remaining_transition_steps = route.steps;
    state.route_owner = None;
    continue_animation_graph_transition_route(asset, clip_ready, controller, state)
}

pub fn transition_clips_ready(
    edge: &AuthoredAnimationGraphEdge,
    ready: impl Fn(&str) -> bool,
) -> bool {
    let mut clips = edge.transition_animation_clip_asset_keys.iter();
    if is_kfm_chain(edge) {
        return clips.all(|clip| ready(clip));
    }
    clips.len() == 0 || clips.any(|clip| ready(clip))
}

fn is_kfm_chain(edge: &AuthoredAnimationGraphEdge) -> bool {
    edge.kfm_transition
        .as_ref()
        .is_some_and(|kfm| kfm.kind == KfmTransitionKind::Chain)
}

pub fn kfm_transition_marker_times<'a>(
    transition: &'a AuthoredKfmTransition,
    source: Option<&'a AuthoredAnimationClipMetadata>,
    target: Option<&'a AuthoredAnimationClipMetadata>,
) -> impl Iterator<Item = [Option<u32>; 2]> + 'a {
    fn marker_time(metadata: Option<&AuthoredAnimationClipMetadata>, name: &str) -> Option<u32> {
        metadata?
            .animation_markers
            .iter()
            .find(|marker| marker.marker_name.eq_ignore_ascii_case(name))
            .map(|marker| marker.event_time_milliseconds)
    }
    transition
        .text_key_pairs
        .iter()
        .filter_map(move |[from, to]| {
            let from_time = from
                .as_deref()
                .map(|key| marker_time(source, key))
                .unwrap_or(Some(0))?;
            let to_time = to
                .as_deref()
                .map(|key| marker_time(target, key))
                .unwrap_or(Some(0))?;
            Some([
                from.as_ref().map(|_| from_time),
                to.as_ref().map(|_| to_time),
            ])
        })
}

fn resolve_route_step(
    asset: &AnimationSetAsset,
    current: u32,
    step: AnimationGraphRouteStep,
) -> Option<(&AuthoredAnimationGraphEdge, &AuthoredAnimationGraphNode)> {
    if step.source_node != current {
        return None;
    }
    let source = asset.animation_graph_nodes.get(current as usize)?;
    let target = asset.animation_graph_nodes.get(step.target_node as usize)?;
    let edge = asset.animation_graph_edges.get(step.edge_index)?;
    (source.animation_graph_node_asset_key == edge.source_animation_graph_node_asset_key
        && target.animation_graph_node_asset_key == edge.target_animation_graph_node_asset_key)
        .then_some((edge, target))
}

pub fn continue_animation_graph_transition_route(
    asset: &AnimationSetAsset,
    clip_ready: impl Fn(&str) -> bool + Copy,
    controller: &mut AnimationPlaybackController,
    state: &mut AnimationGraphPlaybackState,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    while state.active_animation_graph_transition.is_none() {
        let Some(step) = state.remaining_transition_steps.pop_front() else {
            break;
        };
        let remaining = std::mem::take(&mut state.remaining_transition_steps);
        begin_authored_animation_graph_edge_transition(
            asset, clip_ready, step, controller, state, None,
        )?;
        state.remaining_transition_steps = remaining;
    }
    Ok(())
}

fn begin_authored_animation_graph_edge_transition(
    animation_set_asset: &AnimationSetAsset,
    clip_ready: impl Fn(&str) -> bool + Copy,
    step: AnimationGraphRouteStep,
    animation_playback_controller: &mut AnimationPlaybackController,
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_override_milliseconds: Option<u32>,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    let (authored_animation_graph_edge, _) = resolve_route_step(
        animation_set_asset,
        animation_graph_playback_state.current_animation_graph_node_index,
        step,
    )
    .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAuthoredGraphEdge)?;
    let target_animation_graph_node_index = step.target_node;
    let blend_duration_milliseconds = blend_duration_override_milliseconds.unwrap_or_else(|| {
        authored_animation_graph_edge
            .kfm_transition
            .as_ref()
            .map_or_else(
                || {
                    u32::from(
                        interpret_authored_animation_graph_edge_blend_duration_milliseconds(
                            &authored_animation_graph_edge.authored_animation_attributes,
                        ),
                    )
                },
                |transition| {
                    u32::try_from(transition.duration.as_nanos().div_ceil(1_000_000))
                        .unwrap_or(u32::MAX)
                },
            )
    });
    if let Some(authored_transition_animation_clip) = authored_animation_graph_edge
        .transition_animation_clip_asset_keys
        .iter()
        .find(|clip| clip_ready(clip))
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
            .clone_from(authored_transition_animation_clip);
        animation_playback_controller.preserve_seek_on_restart = false;

        let chain_cursor =
            is_kfm_chain(authored_animation_graph_edge).then_some(AnimationGraphChainCursor {
                edge_index: step.edge_index,
                next_clip: 1,
            });
        animation_playback_controller.playback_repetition_policy = if chain_cursor.is_some() {
            AnimationPlaybackRepetitionPolicy::PlayOnce
        } else {
            AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy
        };
        animation_graph_playback_state.active_animation_graph_transition =
            Some(AnimationGraphTransition {
                target_animation_graph_node_index,
                chain_cursor,
            });
        return Ok(());
    }
    enter_animation_graph_node_playback(
        animation_set_asset,
        clip_ready,
        target_animation_graph_node_index,
        animation_playback_controller,
        animation_graph_playback_state,
        blend_duration_milliseconds,
    )
}

/// Starts the next chain clip at a completion boundary and preserves the route.
pub fn advance_animation_graph_chain(
    asset: &AnimationSetAsset,
    controller: &mut AnimationPlaybackController,
    transition: &mut AnimationGraphTransition,
) -> bool {
    let Some(cursor) = transition.chain_cursor.as_mut() else {
        return false;
    };
    let Some(key) = asset
        .animation_graph_edges
        .get(cursor.edge_index)
        .and_then(|edge| {
            edge.transition_animation_clip_asset_keys
                .get(cursor.next_clip)
        })
    else {
        return false;
    };
    cursor.next_clip += 1;
    controller.animation_clip_asset_key.clone_from(key);
    controller.playback_generation = controller.playback_generation.wrapping_add(1);
    controller.preserve_seek_on_restart = false;
    controller.playback_repetition_policy = AnimationPlaybackRepetitionPolicy::PlayOnce;
    true
}

pub fn enter_animation_graph_node_playback(
    animation_set_asset: &AnimationSetAsset,
    clip_ready: impl Fn(&str) -> bool + Copy,
    animation_graph_node_index: u32,
    animation_playback_controller: &mut AnimationPlaybackController,
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_milliseconds: u32,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    let animation_graph_node = animation_set_asset
        .animation_graph_nodes
        .get(animation_graph_node_index as usize)
        .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::UnknownAnimationGraphNode)?;
    let animation_clip_asset_key = select_preferred_animation_clip_from_graph_node(
        animation_set_asset,
        clip_ready,
        animation_graph_node,
    )
    .ok_or(AnimationGraphNodePlaybackRequestRejectionReason::MissingAnimationAsset)?;
    begin_animation_graph_blend_timer(animation_graph_playback_state, blend_duration_milliseconds);
    animation_playback_controller.animation_clip_asset_key = animation_clip_asset_key;
    animation_playback_controller.playback_generation = animation_playback_controller
        .playback_generation
        .wrapping_add(1);
    animation_playback_controller.explicit_clip_request_id = None;
    animation_playback_controller.preserve_seek_on_restart = false;

    animation_playback_controller.playback_repetition_policy =
        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy;
    animation_graph_playback_state.current_animation_graph_node_index = animation_graph_node_index;
    animation_graph_playback_state.active_animation_graph_transition = None;
    animation_graph_playback_state
        .remaining_transition_steps
        .clear();
    animation_graph_playback_state.route_owner = None;
    Ok(())
}

fn begin_animation_graph_blend_timer(
    animation_graph_playback_state: &mut AnimationGraphPlaybackState,
    blend_duration_milliseconds: u32,
) {
    animation_graph_playback_state.requested_blend_duration_milliseconds =
        (blend_duration_milliseconds != 0).then_some(blend_duration_milliseconds);
}

pub fn select_preferred_animation_clip_from_graph_node(
    animation_set_asset: &AnimationSetAsset,
    clip_ready: impl Fn(&str) -> bool + Copy,
    animation_graph_node: &AuthoredAnimationGraphNode,
) -> Option<String> {
    let clips = || {
        animation_graph_node
            .animation_clip_asset_keys
            .iter()
            .filter(|key| clip_ready(key))
    };
    clips()
        .find(|key| key.ends_with("_Idle"))
        .or_else(|| clips().find(|key| key.ends_with("_Ahead")))
        .or_else(|| {
            clips().find(|key| {
                animation_set_asset
                    .find_animation_clip_source_reference_by_asset_key(key)
                    .is_none_or(|reference| {
                        reference.playback_policy.explicit_use_only != Some(true)
                    })
            })
        })
        .or_else(|| clips().next())
        .cloned()
}
