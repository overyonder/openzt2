use bevy::platform::collections::{HashMap, HashSet};
use bevy::{asset::AssetLoadFailedEvent, prelude::*};
use openzt2_game_assets::animation::animation_clip_source_queries::AnimationClipSources;
use openzt2_game_data::AssetId;

use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::{
    ActiveAnimationClipOptions, AnimationClipActionOptions,
};
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphBlend;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphEnabled;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::{
    AnimationPresentationBinder, AnimationPresentations,
};
use openzt2_world_state::animation_graph::model_animation_asset_attachment::resolve_animation_clip_or_graph_node_asset_key;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAssetLibrary;

#[derive(Component)]
pub(super) struct PendingAnimationClipPlayback {
    request: AnimationClipPlaybackRequest,
    request_id: usize,
    source: Handle<AnimationClipSourceAsset>,
    options: Option<AnimationClipActionOptions>,
    ready: bool,
}

#[allow(clippy::too_many_lines)]
pub(super) fn execute_explicit_animation_clip_playback_requests(
    mut commands: Commands,
    clip_sources: AnimationClipSources,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut animation_clip_requests: MessageReader<AnimationClipPlaybackRequest>,
    mut presentation_selections: MessageReader<AnimationClipActionOptions>,
    mut rejected_animation_clip_requests: MessageWriter<AnimationClipPlaybackRequestRejected>,
    mut started_animation_clip_requests: MessageWriter<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted>,
    pending: Query<&PendingAnimationClipPlayback>,
    mut cancellations: MessageReader<openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest>,
    subjects: Query<()>,
    presentations: Query<&AnimationPresentations>,
    presentation_binders: Query<&AnimationPresentationBinder>,
    pending_routes: Query<&super::animation_return_route::AnimationReturnRoute>,
    mut active_players: Query<&mut AnimationPlayer>,
    mut retained_sources: Query<&mut openzt2_world_state::animation_playback::animation_playback_controller_types::RetainedAnimationClipSource>,
    mut animation_playback_controllers: Query<(
        Entity,
        &ModelAnimationAssetLibrary,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&AnimationGraphEnabled>,
        Option<&mut ActiveAnimationClipOptions>,
    )>,
) {
    let cancelled = cancellations.read().copied().collect::<Vec<_>>();
    for cancellation in &cancelled {
        let subject = cancellation.animation_subject_entity;
        let mut controllers = animation_playback_controllers
            .iter_many_mut(AnimationPresentations::for_subject(
                subject,
                presentations.get(subject).ok(),
            ))
            .matched();
        while let Some((entity, _, mut controller, mut graph, _, _)) = controllers.fetch_next() {
            if controller.explicit_clip_request_id == Some(cancellation.request_id) {
                controller.explicit_clip_request_id = None;
                if let Ok(mut player) = active_players.get_mut(entity) {
                    player.pause_all();
                }
            }
            if graph
                .route_owner
                .is_some_and(|owner| owner.request_id == cancellation.request_id)
            {
                graph.route_owner = None;
                graph.remaining_transition_nodes.clear();
                graph.active_animation_graph_transition = None;
                if let Ok(mut player) = active_players.get_mut(entity) {
                    player.pause_all();
                }
            }
            if pending_routes
                .get(entity)
                .is_ok_and(|route| route.request_id == cancellation.request_id)
            {
                commands
                    .entity(entity)
                    .try_remove::<super::animation_return_route::AnimationReturnRoute>();
            }
        }
    }
    let selections = presentation_selections
        .read()
        .map(|options| (options.request_id, *options))
        .collect::<HashMap<_, _>>();
    let mut new_requests = animation_clip_requests
        .read_with_id()
        .map(|(request, id)| (request, id.id, selections.get(&id.id).copied()))
        .collect::<Vec<_>>();
    let new_subjects = new_requests
        .iter()
        .map(|(request, ..)| request.animation_subject_entity)
        .collect::<HashSet<_>>();
    let mut pending_requests = Vec::new();
    for deferred in &pending {
        let subject = deferred.request.animation_subject_entity;
        if new_subjects.contains(&subject) {
            rejected_animation_clip_requests.write(AnimationClipPlaybackRequestRejected {
                request_id: deferred.request_id,
                animation_subject_entity: subject,
                animation_clip_asset_key: deferred.request.animation_clip_asset_key.clone(),
            });
        } else if !cancelled.iter().any(|cancellation| {
            cancellation.animation_subject_entity == subject
                && cancellation.request_id == deferred.request_id
        }) {
            if !deferred.ready {
                continue;
            }
            pending_requests.push((&deferred.request, deferred.request_id, deferred.options));
        }
        commands
            .entity(subject)
            .try_remove::<PendingAnimationClipPlayback>();
    }
    pending_requests.sort_unstable_by_key(|(_, request_id, _)| *request_id);
    let mut seen_subjects = HashSet::<Entity>::default();
    new_requests.reverse();
    new_requests.retain(|(request, request_id, _)| {
        let latest = seen_subjects.insert(request.animation_subject_entity);
        if !latest {
            rejected_animation_clip_requests.write(AnimationClipPlaybackRequestRejected {
                request_id: *request_id,
                animation_subject_entity: request.animation_subject_entity,
                animation_clip_asset_key: request.animation_clip_asset_key.clone(),
            });
        }
        latest
    });
    new_requests.reverse();
    let default_binder = AssetId::from_key("mainobj");
    for (request, request_id, options) in pending_requests.into_iter().chain(new_requests) {
        let subject = request.animation_subject_entity;
        if !subjects.contains(subject)
            || cancelled.iter().any(|cancellation| {
                cancellation.request_id == request_id
                    && cancellation.animation_subject_entity == subject
            })
        {
            continue;
        }
        // A request naming a graph node plays one of the node's clips.
        let candidates = || {
            AnimationPresentations::for_subject(subject, presentations.get(subject).ok()).filter(
                |controller| {
                    options
                        .and_then(|options| options.binder)
                        .is_none_or(|binder| {
                            presentation_binders
                                .get(*controller)
                                .map_or(default_binder, |presentation| presentation.0)
                                == binder
                        })
                },
            )
        };
        let mut animation_clip_request = request.clone();
        if let Some(clip) = animation_playback_controllers
            .iter_many(candidates())
            .matched()
            .flat_map(|(_, library, ..)| library.loaded_model_animation_set_asset_handles.iter())
            .filter_map(|handle| animation_set_assets.get(handle))
            .find_map(|set| {
                resolve_animation_clip_or_graph_node_asset_key(
                    set,
                    &animation_clip_request.animation_clip_asset_key,
                )
            })
        {
            animation_clip_request.animation_clip_asset_key = clip;
        }
        let animation_clip_request = &animation_clip_request;
        let mut started = false;
        let mut controllers = animation_playback_controllers
            .iter_many_mut(candidates())
            .matched();
        while let Some((
            controller_entity,
            animation_library,
            mut animation_playback_controller,
            mut animation_graph_playback_state,
            animation_graph_enabled,
            active_options,
        )) = controllers.fetch_next()
        {
            if animation_graph_enabled.is_some_and(|enabled| !enabled.0) {
                continue;
            }
            let Some(animation_set_asset_handle) = animation_library
                .loaded_model_animation_set_asset_handles
                .iter()
                .find_map(|animation_set_asset_handle| {
                    let animation_set_asset =
                        animation_set_assets.get(animation_set_asset_handle)?;
                    clip_sources.get(
                        animation_set_asset,
                        &animation_clip_request.animation_clip_asset_key,
                    )?;
                    Some(animation_set_asset_handle.clone())
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
                AnimationGraphBlend::from_duration_milliseconds(
                    animation_clip_request.blend_duration_milliseconds,
                );
            if let Some(containing_graph_node_index) = animation_set_asset
                .animation_graph_nodes
                .iter()
                .position(|graph_node| {
                    graph_node.animation_clip_asset_keys.iter().any(|key| {
                        key.eq_ignore_ascii_case(&animation_clip_request.animation_clip_asset_key)
                    })
                })
                .and_then(|index| u32::try_from(index).ok())
            {
                replacement_animation_graph_playback_state.current_animation_graph_node_index =
                    containing_graph_node_index;
            }
            let previous_graph_node = animation_set_assets
                .get(&animation_playback_controller.animation_set_asset)
                .and_then(|set| {
                    set.animation_graph_nodes.get(
                        animation_graph_playback_state.current_animation_graph_node_index as usize,
                    )
                })
                .map(|node| node.animation_graph_node_asset_key.clone());
            if let Some(source) = clip_sources.retained_handle(
                animation_set_asset,
                &animation_clip_request.animation_clip_asset_key,
            ) {
                if let Ok(mut retained) = retained_sources.get_mut(controller_entity) {
                    if retained.0 != source {
                        retained.0 = source;
                    }
                } else {
                    commands.entity(controller_entity).try_insert(openzt2_world_state::animation_playback::animation_playback_controller_types::RetainedAnimationClipSource(source));
                }
            } else {
                commands.entity(controller_entity).try_remove::<openzt2_world_state::animation_playback::animation_playback_controller_types::RetainedAnimationClipSource>();
            }
            match (active_options, options) {
                (Some(mut active), Some(options)) => {
                    active.set_if_neq(ActiveAnimationClipOptions(options));
                }
                (None, Some(options)) => {
                    commands
                        .entity(controller_entity)
                        .try_insert(ActiveAnimationClipOptions(options));
                }
                (Some(_), None) => {
                    commands
                        .entity(controller_entity)
                        .try_remove::<ActiveAnimationClipOptions>();
                }
                (None, None) => {}
            }
            animation_playback_controller.animation_set_asset = animation_set_asset_handle;
            animation_playback_controller.playback_generation = animation_playback_controller
                .playback_generation
                .wrapping_add(1);
            animation_playback_controller.explicit_clip_request_id = Some(request_id);
            animation_playback_controller
                .animation_clip_asset_key
                .clone_from(&animation_clip_request.animation_clip_asset_key);
            animation_playback_controller.preserve_seek_on_restart = false;
            animation_playback_controller.playback_speed_permille =
                animation_clip_request.playback_speed_permille;

            animation_playback_controller.playback_repetition_policy =
                animation_clip_request.playback_repetition_policy;
            *animation_graph_playback_state = replacement_animation_graph_playback_state;
            started_animation_clip_requests.write(openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted {
                animation_subject_entity: animation_clip_request.animation_subject_entity,
                controller_entity,
                request_id,
                playback_generation: animation_playback_controller.playback_generation,
                previous_graph_node,
            });
            started = true;
            break;
        }
        if started {
            continue;
        }
        let loading_source = animation_playback_controllers
            .iter_many(candidates())
            .matched()
            .filter(|(_, _, _, _, enabled, _)| enabled.is_none_or(|enabled| enabled.0))
            .flat_map(|(_, library, ..)| library.loaded_model_animation_set_asset_handles.iter())
            .find_map(|handle| {
                let set = animation_set_assets.get(handle)?;
                let source =
                    clip_sources.request(set, &animation_clip_request.animation_clip_asset_key)?;
                (!clip_sources.failed(&source)).then_some(source)
            });
        if let Some(source) = loading_source {
            commands
                .entity(subject)
                .try_insert(PendingAnimationClipPlayback {
                    request: request.clone(),
                    request_id,
                    ready: clip_sources.resolved(&source),
                    source,
                    options,
                });
            continue;
        }
        {
            warn!(subject = ?animation_clip_request.animation_subject_entity,
                clip = %animation_clip_request.animation_clip_asset_key,
                "explicit animation clip has no ready enabled presentation containing the clip");
            rejected_animation_clip_requests.write(AnimationClipPlaybackRequestRejected {
                request_id,
                animation_subject_entity: animation_clip_request.animation_subject_entity,
                animation_clip_asset_key: request.animation_clip_asset_key.clone(),
            });
        }
    }
}

pub(super) fn wake_animation_clip_requests_after_asset_loading(
    mut loaded: MessageReader<AssetEvent<AnimationClipSourceAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<AnimationClipSourceAsset>>,
    mut pending: Query<&mut PendingAnimationClipPlayback>,
) {
    let finished = loaded
        .read()
        .filter_map(|event| match *event {
            AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .chain(failures.read().map(|failure| failure.id))
        .collect::<HashSet<_>>();
    if finished.is_empty() {
        return;
    }
    for mut request in &mut pending {
        if !request.ready && finished.contains(&request.source.id()) {
            request.ready = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

    #[test]
    fn missing_presentation_rejects_the_exact_requested_clip() {
        let mut application = App::new();
        application
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<AnimationSetAsset>()
            .init_asset::<AnimationClipSourceAsset>()
            .add_message::<AnimationClipPlaybackRequest>()
            .add_message::<AnimationClipActionOptions>()
            .add_message::<AnimationClipPlaybackRequestRejected>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted>()
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
