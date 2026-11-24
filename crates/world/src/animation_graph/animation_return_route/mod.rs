use super::animation_graph_route_selection::resolve_authored_animation_route;
use bevy::prelude::*;
use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationReturnRouteOutcome, AnimationReturnRouteRequest,
        },
        animation_graph_playback_state_types::{
            AnimationGraphPlaybackState, AnimationGraphRoute, AnimationGraphRouteOwner,
        },
        animation_graph_transition_operations::begin_resolved_animation_graph_route,
    },
    animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, RetainedAnimationClipSource,
    },
};

#[derive(Component)]
pub(super) struct AnimationReturnRoute {
    pub(super) request_id: usize,
    subject: Entity,
    generation: u64,
    node: String,
    sources: Vec<Handle<AnimationClipSourceAsset>>,
    prepared: Option<AnimationGraphRoute>,
}

pub(super) fn receive_return_route_requests(
    mut commands: Commands,
    mut requests: MessageReader<AnimationReturnRouteRequest>,
    mut controllers: Query<(
        &AnimationPlaybackController,
        Option<&mut AnimationReturnRoute>,
    )>,
) {
    for request in requests.read() {
        let Ok((controller, route)) = controllers.get_mut(request.controller) else {
            commands.trigger(AnimationReturnRouteOutcome {
                subject: request.subject,
                request_id: request.request_id,
                succeeded: false,
            });
            continue;
        };
        if controller.explicit_clip_request_id != Some(request.request_id)
            || controller.playback_generation != request.generation
        {
            commands.trigger(AnimationReturnRouteOutcome {
                subject: request.subject,
                request_id: request.request_id,
                succeeded: false,
            });
            continue;
        }
        let wanted = AnimationReturnRoute {
            request_id: request.request_id,
            subject: request.subject,
            generation: request.generation,
            node: request.node.clone(),
            sources: Vec::new(),
            prepared: None,
        };
        if let Some(mut route) = route {
            *route = wanted;
        } else {
            commands.entity(request.controller).try_insert(wanted);
        }
    }
}

pub(super) fn advance_return_routes(
    mut commands: Commands,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: AnimationClipSources,
    subjects: Query<()>,
    mut loaded: MessageReader<AssetEvent<AnimationClipSourceAsset>>,
    mut failed: MessageReader<AssetLoadFailedEvent<AnimationClipSourceAsset>>,
    mut routes: Query<(
        Entity,
        &mut AnimationReturnRoute,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&mut RetainedAnimationClipSource>,
    )>,
) {
    let finished = loaded
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .chain(failed.read().map(|failure| failure.id))
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, mut route, mut controller, mut state, retained) in &mut routes {
        if !subjects.contains(route.subject) {
            commands.entity(entity).try_remove::<AnimationReturnRoute>();
            continue;
        }
        if !route.is_changed()
            && !controller.is_changed()
            && !route
                .sources
                .iter()
                .any(|source| finished.contains(&source.id()))
        {
            continue;
        }
        let generation = controller.playback_generation;
        let outcome = advance_route(
            route.bypass_change_detection(),
            &mut controller,
            &mut state,
            &sets,
            &clips,
        );
        if generation != controller.playback_generation {
            let source = sets
                .get(&controller.animation_set_asset)
                .and_then(|set| clips.retained_handle(set, &controller.animation_clip_asset_key));
            if let Some(source) = source {
                if let Some(mut retained) = retained {
                    if retained.0 != source {
                        retained.0 = source;
                    }
                } else {
                    commands
                        .entity(entity)
                        .try_insert(RetainedAnimationClipSource(source));
                }
            }
        }
        if let Some(succeeded) = outcome {
            commands.trigger(AnimationReturnRouteOutcome {
                subject: route.subject,
                request_id: route.request_id,
                succeeded,
            });
            commands.entity(entity).try_remove::<AnimationReturnRoute>();
        }
    }
}

fn advance_route(
    route: &mut AnimationReturnRoute,
    controller: &mut Mut<AnimationPlaybackController>,
    state: &mut Mut<AnimationGraphPlaybackState>,
    sets: &Assets<AnimationSetAsset>,
    clips: &AnimationClipSources,
) -> Option<bool> {
    if let Some(owner) = state.route_owner.filter(|owner| {
        owner.request_id == route.request_id
            && owner.playback_generation == controller.playback_generation
    }) {
        if state.active_animation_graph_transition.is_some() {
            return None;
        }
        return Some(
            state.remaining_transition_steps.is_empty()
                && state.current_animation_graph_node_index == owner.target_node,
        );
    }
    if controller.explicit_clip_request_id != Some(route.request_id)
        || controller.playback_generation != route.generation
    {
        return Some(false);
    }
    if route
        .sources
        .iter()
        .any(|source| !clips.resolved(source) && !clips.failed(source))
    {
        return None;
    }
    let Some(set) = sets.get(&controller.animation_set_asset) else {
        return Some(false);
    };
    let Some(target) = set
        .find_animation_graph_node_index_by_asset_key(&route.node)
        .and_then(|index| u32::try_from(index).ok())
    else {
        return Some(false);
    };
    if route.prepared.is_none() {
        route.prepared =
            resolve_authored_animation_route(set, state.current_animation_graph_node_index, target);
    }
    let Some(path) = route.prepared.as_ref() else {
        return Some(false);
    };
    let mut loading = false;
    let same_node = path.steps.is_empty();
    for (next, edge) in path
        .steps
        .iter()
        .map(|step| {
            (
                step.target_node,
                set.animation_graph_edges.get(step.edge_index),
            )
        })
        .chain(same_node.then_some((target, None)))
    {
        let Some(target_node) = set.animation_graph_nodes.get(next as usize) else {
            return Some(false);
        };
        for choices in edge
            .into_iter()
            .map(|edge| &edge.transition_animation_clip_asset_keys)
            .filter(|choices| !choices.is_empty())
            .chain(std::iter::once(&target_node.animation_clip_asset_keys))
        {
            if choices.iter().any(|key| clips.get(set, key).is_some()) {
                continue;
            }
            let Some(source) = choices.iter().find_map(|key| {
                let source = clips.request(set, key)?;
                (!clips.failed(&source)).then_some(source)
            }) else {
                return Some(false);
            };
            if !route.sources.contains(&source) {
                route.sources.push(source);
            }
            loading = true;
        }
    }
    if loading {
        return None;
    }
    let path = route.prepared.take()?;
    if begin_resolved_animation_graph_route(
        set,
        |key| clips.get(set, key).is_some(),
        path,
        controller,
        state,
        None,
    )
    .is_err()
    {
        return Some(false);
    }
    state.route_owner = Some(AnimationGraphRouteOwner {
        request_id: route.request_id,
        target_node: target,
        playback_generation: controller.playback_generation,
    });
    if state.active_animation_graph_transition.is_some() {
        None
    } else {
        Some(
            state.remaining_transition_steps.is_empty()
                && state.current_animation_graph_node_index == target,
        )
    }
}
