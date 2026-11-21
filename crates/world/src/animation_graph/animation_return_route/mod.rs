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
            AnimationGraphPlaybackState, AnimationGraphRouteOwner,
        },
        animation_graph_transition_operations::{
            begin_requested_animation_graph_node_transition, find_animation_graph_transition_route,
        },
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
    mut routes: Query<(
        Entity,
        &mut AnimationReturnRoute,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&mut RetainedAnimationClipSource>,
    )>,
) {
    for (entity, mut route, mut controller, mut state, retained) in &mut routes {
        if !subjects.contains(route.subject) {
            commands.entity(entity).try_remove::<AnimationReturnRoute>();
            continue;
        }
        let generation = controller.playback_generation;
        let outcome = advance_route(&mut route, &mut controller, &mut state, &sets, &clips);
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
    controller: &mut AnimationPlaybackController,
    state: &mut AnimationGraphPlaybackState,
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
            state.remaining_transition_nodes.is_empty()
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
    let Some(path) = find_animation_graph_transition_route(
        set,
        state.current_animation_graph_node_index,
        target,
    ) else {
        return Some(false);
    };

    let mut from = state.current_animation_graph_node_index;
    let mut loading = false;
    let mut nodes = path.iter().copied().peekable();
    let same_node = nodes.peek().is_none();
    for next in nodes.chain(same_node.then_some(target)) {
        let target_node = &set.animation_graph_nodes[next as usize];
        let edge = (from != next)
            .then(|| {
                let source_node = &set.animation_graph_nodes[from as usize];
                set.animation_graph_edges.iter().find(|edge| {
                    edge.source_animation_graph_node_asset_key
                        == source_node.animation_graph_node_asset_key
                        && edge.target_animation_graph_node_asset_key
                            == target_node.animation_graph_node_asset_key
                })
            })
            .flatten();
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
        from = next;
    }
    if loading {
        return None;
    }
    if begin_requested_animation_graph_node_transition(
        set,
        |key| clips.get(set, key).is_some(),
        target,
        controller,
        state,
        None,
        false,
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
            state.remaining_transition_nodes.is_empty()
                && state.current_animation_graph_node_index == target,
        )
    }
}
