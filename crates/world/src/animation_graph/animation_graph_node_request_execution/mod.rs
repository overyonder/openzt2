use crate::animation_playback::managed_clip_binding::ManagedAnimationClip;
use bevy::{
    animation::ActiveAnimation, asset::AssetLoadFailedEvent, platform::collections::HashSet,
    prelude::*,
};
use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationGraphNodePlaybackRequest, AnimationGraphNodePlaybackRequestRejected,
        },
        animation_graph_playback_state_types::{
            AnimationGraphEnabled, AnimationGraphPlaybackState, AnimationGraphRoute,
        },
        animation_graph_transition_operations::enter_animation_graph_node_playback,
        animation_presentation_relationship_types::AnimationPresentations,
        model_animation_asset_binding_types::ModelAnimationAssetLibrary,
    },
    animation_playback::animation_playback_controller_types::AnimationPlaybackController,
};
use std::time::Duration;

use super::animation_graph_route_selection::{
    authored_kfm_duration, begin_prepared_route, crossed_forward_marker, current_route_animation,
    kfm_route_markers, prepare_authored_route_clips, publish_transition_options,
    resolve_authored_animation_route, retain_prepared_continuation, GraphTransitionOptions,
    RouteClipReadiness,
};

#[derive(Component)]
pub(super) struct PendingNodeRoute {
    subject: Entity,
    source_set: Handle<AnimationSetAsset>,
    target_set: Handle<AnimationSetAsset>,
    generation: u64,
    route: AnimationGraphRoute,
    direct: bool,
    blend: Option<u32>,
    preserve_seek: bool,
    clips: Vec<Handle<AnimationClipSourceAsset>>,
    marker_wait: Option<(f32, Option<f32>, f32)>,
}

pub(super) fn execute_animation_graph_node_playback_requests<
    Lane: Component,
    const RENDER: bool,
>(
    mut commands: Commands,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: AnimationClipSources,
    mut requests: MessageReader<AnimationGraphNodePlaybackRequest>,
    mut rejected: MessageWriter<AnimationGraphNodePlaybackRequestRejected>,
    presentations: Query<&AnimationPresentations>,
    all_controllers: Query<(), With<AnimationPlaybackController>>,
    mut controllers: Query<
        (
            Entity,
            &ModelAnimationAssetLibrary,
            &mut AnimationPlaybackController,
            &mut AnimationGraphPlaybackState,
            Option<&AnimationGraphEnabled>,
            Option<&AnimationPlayer>,
            Option<&ManagedAnimationClip>,
            Option<&mut PendingNodeRoute>,
            Option<&mut GraphTransitionOptions>,
        ),
        With<Lane>,
    >,
) {
    for request in requests.read() {
        let subject = request.animation_subject_entity;
        let candidates =
            || AnimationPresentations::for_subject(subject, presentations.get(subject).ok());
        let mut affected = controllers.iter_many_mut(candidates()).matched();
        let mut seen = false;
        let mut accepted = false;
        while let Some((
            entity,
            library,
            mut controller,
            mut state,
            enabled,
            player,
            binding,
            pending,
            seek,
        )) = affected.fetch_next()
        {
            seen = true;
            let planned = select_node_route(request, &sets, library, &controller, &state)
                .filter(|_| enabled.is_none_or(|enabled| enabled.0));
            let Some(mut route) = planned else {
                commands.entity(entity).try_remove::<PendingNodeRoute>();
                continue;
            };
            match advance_node_route(
                &mut route,
                &sets,
                &clips,
                current_route_animation(player, binding, controller.playback_generation),
                &mut controller,
                &mut state,
            ) {
                NodeRouteProgress::Complete(options) => {
                    retain_prepared_continuation(&mut commands, entity, &state);
                    publish_transition_options(
                        &mut commands,
                        entity,
                        seek,
                        controller.playback_generation,
                        options,
                    );
                    commands.entity(entity).try_remove::<PendingNodeRoute>();
                    accepted = true;
                }
                NodeRouteProgress::Waiting => {
                    if let Some(mut pending) = pending {
                        *pending = route;
                    } else {
                        commands.entity(entity).try_insert(route);
                    }
                    accepted = true;
                }
                NodeRouteProgress::Rejected => {
                    commands.entity(entity).try_remove::<PendingNodeRoute>();
                }
            }
        }
        if !accepted
            && (seen || !RENDER && !candidates().any(|entity| all_controllers.contains(entity)))
        {
            rejected.write(AnimationGraphNodePlaybackRequestRejected {
                animation_subject_entity: subject,
            });
        }
    }
}

fn select_node_route(
    request: &AnimationGraphNodePlaybackRequest,
    sets: &Assets<AnimationSetAsset>,
    library: &ModelAnimationAssetLibrary,
    controller: &AnimationPlaybackController,
    state: &AnimationGraphPlaybackState,
) -> Option<PendingNodeRoute> {
    let current = &controller.animation_set_asset;
    let (target_set, target) = std::iter::once(current)
        .chain(
            library
                .loaded_model_animation_set_asset_handles
                .iter()
                .filter(|handle| *handle != current),
        )
        .find_map(|handle| {
            let index = sets
                .get(handle)?
                .find_animation_graph_node_index_by_asset_key(
                    &request.animation_graph_node_asset_key,
                )?;
            Some((handle, u32::try_from(index).ok()?))
        })?;
    let asset = sets.get(target_set)?;
    let source = state.current_animation_graph_node_index;
    let direct = target_set != current
        || target == source
        || request.enters_immediately
        || controller.animation_clip_asset_key.is_empty();
    let route = if direct {
        resolve_authored_animation_route(asset, target, target)?
    } else {
        if state.active_animation_graph_transition.is_some() {
            return None;
        }
        resolve_authored_animation_route(asset, source, target)?
    };
    Some(PendingNodeRoute {
        subject: request.animation_subject_entity,
        source_set: current.clone(),
        target_set: target_set.clone(),
        generation: controller.playback_generation,
        route,
        direct,
        blend: request.blend_duration_milliseconds,
        preserve_seek: request.preserves_current_playback_time,
        clips: Vec::new(),
        marker_wait: None,
    })
}

pub(super) fn resume_animation_graph_node_routes<Lane: Component>(
    mut commands: Commands,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: AnimationClipSources,
    mut ready: MessageReader<AssetEvent<AnimationClipSourceAsset>>,
    mut failed: MessageReader<AssetLoadFailedEvent<AnimationClipSourceAsset>>,
    mut replaced: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut rejected: MessageWriter<AnimationGraphNodePlaybackRequestRejected>,
    mut routes: Query<
        (
            Entity,
            &mut PendingNodeRoute,
            &mut AnimationPlaybackController,
            &mut AnimationGraphPlaybackState,
            Option<&AnimationPlayer>,
            Option<&ManagedAnimationClip>,
            Option<&mut GraphTransitionOptions>,
        ),
        With<Lane>,
    >,
) {
    let clips_changed = ready
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .chain(failed.read().map(|event| event.id))
        .collect::<HashSet<_>>();
    let sets_changed = replaced
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    for (entity, mut route, mut controller, mut state, player, binding, seek) in &mut routes {
        let invalid = route.generation != controller.playback_generation
            || route.source_set != controller.animation_set_asset
            || !route.direct && route.route.source_node != state.current_animation_graph_node_index
            || sets_changed.contains(&route.target_set.id())
            || sets_changed.contains(&route.source_set.id());
        let progress = if invalid {
            NodeRouteProgress::Rejected
        } else if route.marker_wait.is_some()
            || route
                .clips
                .iter()
                .any(|clip| clips_changed.contains(&clip.id()))
        {
            advance_node_route(
                &mut route,
                &sets,
                &clips,
                current_route_animation(player, binding, controller.playback_generation),
                &mut controller,
                &mut state,
            )
        } else {
            continue;
        };
        match progress {
            NodeRouteProgress::Complete(options) => {
                retain_prepared_continuation(&mut commands, entity, &state);
                publish_transition_options(
                    &mut commands,
                    entity,
                    seek,
                    controller.playback_generation,
                    options,
                );
            }
            NodeRouteProgress::Rejected => {
                rejected.write(AnimationGraphNodePlaybackRequestRejected {
                    animation_subject_entity: route.subject,
                });
            }
            NodeRouteProgress::Waiting => continue,
        }
        commands.entity(entity).try_remove::<PendingNodeRoute>();
    }
}

enum NodeRouteProgress {
    Complete((Option<f32>, Option<Duration>)),
    Waiting,
    Rejected,
}

fn advance_node_route(
    route: &mut PendingNodeRoute,
    sets: &Assets<AnimationSetAsset>,
    clips: &AnimationClipSources,
    playing: Option<&ActiveAnimation>,
    controller: &mut AnimationPlaybackController,
    state: &mut AnimationGraphPlaybackState,
) -> NodeRouteProgress {
    let Some(asset) = sets.get(&route.target_set) else {
        return NodeRouteProgress::Rejected;
    };
    match prepare_authored_route_clips(asset, clips, &route.route, &mut route.clips) {
        RouteClipReadiness::Loading => return NodeRouteProgress::Waiting,
        RouteClipReadiness::Unavailable => return NodeRouteProgress::Rejected,
        RouteClipReadiness::Ready => {}
    }
    let mut target_seek = None;
    if !route.direct {
        let time = playing.map_or(0.0, |active| active.seek_time());
        if let Some((marker, target, last)) = route.marker_wait {
            if playing.is_some_and(|active| !active.is_paused() && !active.is_finished())
                && !crossed_forward_marker(last, time, marker)
            {
                route.marker_wait = Some((marker, target, time));
                return NodeRouteProgress::Waiting;
            }
            target_seek = target;
        } else if let Some((marker, target)) = kfm_route_markers(
            asset,
            clips,
            &route.route,
            &controller.animation_clip_asset_key,
            time,
        ) {
            if let Some(marker) = marker.filter(|&marker| {
                marker != time
                    && playing.is_some_and(|active| !active.is_paused() && !active.is_finished())
            }) {
                route.marker_wait = Some((marker, target, time));
                return NodeRouteProgress::Waiting;
            }
            target_seek = target;
        }
    }
    let clip_ready = |key: &str| clips.get(asset, key).is_some();
    let result = if route.direct {
        enter_animation_graph_node_playback(
            asset,
            clip_ready,
            route.route.target_node,
            controller,
            state,
            route.blend.unwrap_or_default(),
        )
    } else {
        begin_prepared_route(
            asset,
            clip_ready,
            route.route.clone(),
            controller,
            state,
            route.blend,
        )
    };
    if result.is_err() {
        return NodeRouteProgress::Rejected;
    }
    controller.animation_set_asset = route.target_set.clone();
    controller.preserve_seek_on_restart = route.preserve_seek && target_seek.is_none();
    let duration = if route.direct || route.blend.is_some() {
        None
    } else {
        authored_kfm_duration(asset, &route.route)
    };
    NodeRouteProgress::Complete((target_seek, duration))
}
