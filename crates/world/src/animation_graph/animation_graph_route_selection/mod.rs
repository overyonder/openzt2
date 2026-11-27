use crate::animation_playback::managed_clip_binding::ManagedAnimationClip;
use bevy::{animation::ActiveAnimation, prelude::*};
use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_game_data::animation::animation_set::KfmTransitionKind;
use openzt2_world_state::animation_graph::{
    animation_graph_playback_message_types::AnimationGraphNodePlaybackRequestRejectionReason,
    animation_graph_playback_state_types::{
        AnimationGraphPlaybackState, AnimationGraphRoute, AnimationGraphRouteStep,
    },
    animation_graph_transition_operations::{
        begin_resolved_animation_graph_route, kfm_transition_marker_times,
        resolve_animation_graph_route, select_preferred_animation_clip_from_graph_node,
    },
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use std::time::Duration;

pub(super) fn resolve_authored_animation_route(
    asset: &AnimationSetAsset,
    source: u32,
    target: u32,
) -> Option<AnimationGraphRoute> {
    resolve_animation_graph_route(
        asset.animation_graph_nodes.len(),
        |node| asset.outgoing_graph_edges(node),
        source,
        target,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteClipReadiness {
    Ready,
    Loading,
    Unavailable,
}

pub(super) fn prepare_authored_route_clips(
    set: &AnimationSetAsset,
    clips: &AnimationClipSources,
    route: &AnimationGraphRoute,
    requested: &mut Vec<Handle<AnimationClipSourceAsset>>,
) -> RouteClipReadiness {
    let mut loading = false;
    let mut request_choices = |choices: &[String]| {
        if choices.iter().any(|key| clips.get(set, key).is_some()) {
            return true;
        }
        let Some(handle) = choices.iter().find_map(|key| {
            let handle = clips.request(set, key)?;
            (!clips.failed(&handle)).then_some(handle)
        }) else {
            return false;
        };
        if !requested.contains(&handle) {
            requested.push(handle);
        }
        loading = true;
        true
    };
    for step in &route.steps {
        let Some(edge) = set.animation_graph_edges.get(step.edge_index) else {
            return RouteClipReadiness::Unavailable;
        };
        let choices = &edge.transition_animation_clip_asset_keys;
        if edge
            .kfm_transition
            .as_ref()
            .is_some_and(|transition| transition.kind == KfmTransitionKind::Chain)
        {
            if !choices
                .iter()
                .all(|key| request_choices(std::slice::from_ref(key)))
            {
                return RouteClipReadiness::Unavailable;
            }
        } else if !choices.is_empty() && !request_choices(choices) {
            return RouteClipReadiness::Unavailable;
        }
        if !set
            .animation_graph_nodes
            .get(step.target_node as usize)
            .is_some_and(|target| request_choices(&target.animation_clip_asset_keys))
        {
            return RouteClipReadiness::Unavailable;
        }
    }
    if route.steps.is_empty()
        && !set
            .animation_graph_nodes
            .get(route.target_node as usize)
            .is_some_and(|target| request_choices(&target.animation_clip_asset_keys))
    {
        return RouteClipReadiness::Unavailable;
    }
    if loading {
        RouteClipReadiness::Loading
    } else {
        RouteClipReadiness::Ready
    }
}

#[derive(Component, PartialEq)]
pub(crate) struct GraphTransitionOptions {
    pub(crate) generation: u64,
    pub(crate) seek: Option<f32>,
    pub(crate) duration: Option<Duration>,
}

pub(super) fn authored_kfm_duration(
    set: &AnimationSetAsset,
    route: &AnimationGraphRoute,
) -> Option<Duration> {
    let first = route.steps.front()?;
    set.animation_graph_edges
        .get(first.edge_index)?
        .kfm_transition
        .as_ref()
        .map(|transition| transition.duration)
}

pub(super) fn publish_transition_options(
    commands: &mut Commands,
    entity: Entity,
    existing: Option<Mut<GraphTransitionOptions>>,
    generation: u64,
    (seek, duration): (Option<f32>, Option<Duration>),
) {
    if seek.is_some() || duration.is_some() {
        let next = GraphTransitionOptions {
            generation,
            seek,
            duration,
        };
        if let Some(mut current) = existing {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).try_insert(next);
        }
    } else if existing.is_some() {
        commands
            .entity(entity)
            .try_remove::<GraphTransitionOptions>();
    }
}

pub(super) fn kfm_route_markers(
    asset: &AnimationSetAsset,
    clips: &AnimationClipSources,
    route: &AnimationGraphRoute,
    source_clip: &str,
    source_seek: f32,
) -> Option<(Option<f32>, Option<f32>)> {
    let step = route.steps.front()?;
    let edge = asset.animation_graph_edges.get(step.edge_index)?;
    let transition = edge.kfm_transition.as_ref()?;
    if transition.kind == KfmTransitionKind::Chain {
        return None;
    }
    let source = clips.get(asset, source_clip);
    let target_node = asset.animation_graph_nodes.get(step.target_node as usize)?;
    let target = select_preferred_animation_clip_from_graph_node(
        asset,
        |key| clips.get(asset, key).is_some(),
        target_node,
    )
    .and_then(|key| clips.get(asset, &key));
    kfm_transition_marker_times(
        transition,
        source.as_ref().and_then(|clip| clip.metadata),
        target.as_ref().and_then(|clip| clip.metadata),
    )
    .map(|[from, to]| {
        (
            from.map(|ms| ms as f32 / 1000.0),
            to.map(|ms| ms as f32 / 1000.0),
        )
    })
    .min_by(|(left, _), (right, _)| {
        let priority = |time: Option<f32>| {
            let time = time.unwrap_or(source_seek);
            (time < source_seek, time)
        };
        let (left_wrapped, left_time) = priority(*left);
        let (right_wrapped, right_time) = priority(*right);
        left_wrapped
            .cmp(&right_wrapped)
            .then_with(|| left_time.total_cmp(&right_time))
    })
}

pub(super) fn current_route_animation<'a>(
    player: Option<&'a AnimationPlayer>,
    clip: Option<&ManagedAnimationClip>,
    generation: u64,
) -> Option<&'a ActiveAnimation> {
    let binding = clip.filter(|clip| clip.generation == generation)?;
    player?.animation(binding.node)
}

pub(super) fn crossed_forward_marker(previous: f32, current: f32, marker: f32) -> bool {
    if previous <= current {
        previous < marker && marker <= current
    } else {
        previous < marker || marker <= current
    }
}

pub(super) fn has_kfm_steps<'a>(
    asset: &AnimationSetAsset,
    steps: impl IntoIterator<Item = &'a AnimationGraphRouteStep>,
) -> bool {
    steps.into_iter().any(|step| {
        asset
            .animation_graph_edges
            .get(step.edge_index)
            .is_some_and(|edge| edge.kfm_transition.is_some())
    })
}

pub(super) fn begin_prepared_route(
    set: &AnimationSetAsset,
    ready: impl Fn(&str) -> bool + Copy,
    mut route: AnimationGraphRoute,
    controller: &mut AnimationPlaybackController,
    state: &mut AnimationGraphPlaybackState,
    duration: Option<u32>,
) -> Result<(), AnimationGraphNodePlaybackRequestRejectionReason> {
    if route.steps.len() <= 1 || !has_kfm_steps(set, &route.steps) {
        return begin_resolved_animation_graph_route(
            set, ready, route, controller, state, duration,
        );
    }
    let step = route
        .steps
        .pop_front()
        .expect("a route with KFM steps has a first step");
    let first = AnimationGraphRoute {
        source_node: route.source_node,
        target_node: step.target_node,
        steps: [step].into(),
    };
    begin_resolved_animation_graph_route(set, ready, first, controller, state, duration)?;
    state.remaining_transition_steps = route.steps;
    Ok(())
}

#[derive(Component)]
pub(super) struct PreparedKfmContinuation;

pub(super) fn retain_prepared_continuation(
    commands: &mut Commands,
    entity: Entity,
    state: &AnimationGraphPlaybackState,
) {
    if !state.remaining_transition_steps.is_empty() {
        commands
            .entity(entity)
            .try_insert_if_new(PreparedKfmContinuation);
    }
}

#[derive(Component)]
pub(super) struct WaitingForKfmMarker {
    generation: u64,
    last_seek: f32,
    source: f32,
    target: Option<f32>,
}

pub(super) fn continue_prepared_kfm_routes<Lane: Component>(
    mut commands: Commands,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: AnimationClipSources,
    mut reloaded: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut actors: Query<
        (
            Entity,
            &mut AnimationPlaybackController,
            &mut AnimationGraphPlaybackState,
            Option<&AnimationPlayer>,
            Option<&ManagedAnimationClip>,
            Option<&mut WaitingForKfmMarker>,
            Option<&mut GraphTransitionOptions>,
        ),
        (With<Lane>, With<PreparedKfmContinuation>),
    >,
) {
    let reloaded = reloaded
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, mut controller, mut state, player, binding, mut waiting, output_seek) in
        &mut actors
    {
        if reloaded.contains(&controller.animation_set_asset.id()) {
            state.remaining_transition_steps.clear();
            state.active_animation_graph_transition = None;
            commands
                .entity(entity)
                .try_remove::<(WaitingForKfmMarker, PreparedKfmContinuation)>();
            continue;
        }
        if state.active_animation_graph_transition.is_some() {
            continue;
        }
        let Some(&step) = state.remaining_transition_steps.front() else {
            commands
                .entity(entity)
                .try_remove::<(WaitingForKfmMarker, PreparedKfmContinuation)>();
            continue;
        };
        let Some(set) = sets.get(&controller.animation_set_asset) else {
            continue;
        };
        if step.source_node != state.current_animation_graph_node_index {
            state.remaining_transition_steps.clear();
            commands
                .entity(entity)
                .try_remove::<(WaitingForKfmMarker, PreparedKfmContinuation)>();
            continue;
        }
        let route = AnimationGraphRoute {
            source_node: step.source_node,
            target_node: step.target_node,
            steps: [step].into(),
        };
        let playing = current_route_animation(player, binding, controller.playback_generation);
        if playing.is_none() && player.is_some() && !controller.animation_clip_asset_key.is_empty()
        {
            continue;
        }
        let seek = playing.map_or(0.0, |active| active.seek_time());
        let advancing = playing.is_some_and(|active| !active.is_paused() && !active.is_finished());
        let mut wanted_seek = None;
        if let Some(waiting) = waiting
            .as_mut()
            .filter(|waiting| waiting.generation == controller.playback_generation)
        {
            if advancing && !crossed_forward_marker(waiting.last_seek, seek, waiting.source) {
                if waiting.last_seek != seek {
                    waiting.last_seek = seek;
                }
                continue;
            }
            wanted_seek = waiting.target;
        } else if let Some((marker, target)) = kfm_route_markers(
            set,
            &clips,
            &route,
            &controller.animation_clip_asset_key,
            seek,
        ) {
            if let Some(source) = marker.filter(|&marker| advancing && marker != seek) {
                let next = WaitingForKfmMarker {
                    generation: controller.playback_generation,
                    last_seek: seek,
                    source,
                    target,
                };
                if let Some(mut current) = waiting {
                    *current = next;
                } else {
                    commands.entity(entity).try_insert(next);
                }
                continue;
            }
            wanted_seek = target;
        }
        commands.entity(entity).try_remove::<WaitingForKfmMarker>();
        let mut remaining = std::mem::take(&mut state.remaining_transition_steps);
        remaining.pop_front();
        let owner = state.route_owner;
        let duration = authored_kfm_duration(set, &route);
        if begin_resolved_animation_graph_route(
            set,
            |key| clips.get(set, key).is_some(),
            route,
            &mut controller,
            &mut state,
            None,
        )
        .is_ok()
        {
            state.remaining_transition_steps = remaining;
            if state.remaining_transition_steps.is_empty() {
                commands
                    .entity(entity)
                    .try_remove::<PreparedKfmContinuation>();
            }
            state.route_owner = owner.map(|mut owner| {
                owner.playback_generation = controller.playback_generation;
                owner
            });
            publish_transition_options(
                &mut commands,
                entity,
                output_seek,
                controller.playback_generation,
                (wanted_seek, duration),
            );
        }
    }
}
