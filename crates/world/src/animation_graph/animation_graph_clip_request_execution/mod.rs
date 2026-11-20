use bevy::platform::collections::{HashMap, HashSet};
use bevy::{asset::LoadState, prelude::*};
use openzt2_game_data::animation::animation_set::AuthoredAnimationGraphNode;
use openzt2_game_data::AssetId;

use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackState;

use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPresentationSelection;
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
    binder: Option<AssetId>,
}

#[allow(clippy::too_many_lines)]
pub(super) fn execute_explicit_animation_clip_playback_requests(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    animation_clip_sources: Res<Assets<AnimationClipSourceAsset>>,
    mut animation_set_assets: ResMut<Assets<AnimationSetAsset>>,
    mut animation_clip_requests: MessageReader<AnimationClipPlaybackRequest>,
    mut presentation_selections: MessageReader<AnimationClipPresentationSelection>,
    mut rejected_animation_clip_requests: MessageWriter<AnimationClipPlaybackRequestRejected>,
    mut started_animation_clip_requests: MessageWriter<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted>,
    pending: Query<&PendingAnimationClipPlayback>,
    mut cancellations: MessageReader<openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest>,
    subjects: Query<()>,
    presentations: Query<&AnimationPresentations>,
    presentation_binders: Query<&AnimationPresentationBinder>,
    mut animation_playback_controllers: Query<(
        &ModelAnimationAssetLibrary,
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&AnimationGraphEnabled>,
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
        while let Some((_, mut controller, _, _)) = controllers.fetch_next() {
            if controller.explicit_clip_request_id == Some(cancellation.request_id) {
                controller.explicit_clip_request_id = None;
                controller.playback_generation = controller.playback_generation.wrapping_add(1);
                controller.playback_state = AnimationPlaybackState::Finished;
            }
        }
    }
    let selections = presentation_selections
        .read()
        .map(|selection| (selection.request_id, selection.binder))
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
            if !animation_clip_sources.contains(deferred.source.id())
                && !matches!(
                    asset_server.load_state(deferred.source.id()),
                    LoadState::Failed(_)
                )
            {
                continue;
            }
            pending_requests.push((&deferred.request, deferred.request_id, deferred.binder));
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
    for (request, request_id, binder) in pending_requests.into_iter().chain(new_requests) {
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
                    binder.is_none_or(|binder| {
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
            .flat_map(|(library, ..)| library.loaded_model_animation_set_asset_handles.iter())
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
            animation_library,
            mut animation_playback_controller,
            mut animation_graph_playback_state,
            animation_graph_enabled,
        )) = controllers.fetch_next()
        {
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
                        fractional_milliseconds: default(),
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
            let previous_graph_node = animation_set_assets
                .get(&animation_playback_controller.animation_set_asset)
                .and_then(|set| {
                    set.animation_graph_nodes.get(
                        animation_graph_playback_state.current_animation_graph_node_index as usize,
                    )
                })
                .map(|node| node.animation_graph_node_asset_key.clone());
            animation_playback_controller.animation_set_asset = animation_set_asset_handle;
            animation_playback_controller.playback_generation = animation_playback_controller
                .playback_generation
                .wrapping_add(1);
            animation_playback_controller.explicit_clip_request_id = Some(request_id);
            animation_playback_controller
                .animation_clip_asset_key
                .clone_from(&animation_clip_request.animation_clip_asset_key);
            animation_playback_controller.elapsed_milliseconds = 0;
            animation_playback_controller
                .fractional_milliseconds
                .clear();
            animation_playback_controller.playback_speed_permille =
                animation_clip_request.playback_speed_permille;
            animation_playback_controller.playback_state = AnimationPlaybackState::Playing;
            animation_playback_controller.playback_repetition_policy =
                animation_clip_request.playback_repetition_policy;
            *animation_graph_playback_state = replacement_animation_graph_playback_state;
            started_animation_clip_requests.write(openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted {
                animation_subject_entity: animation_clip_request.animation_subject_entity,
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
        // Start loading a clip the subject's manifest names; the request
        // retries each update until the clip loads or its load fails.
        let loading_source = animation_playback_controllers
            .iter_many(candidates())
            .matched()
            .filter(|(_, _, _, enabled)| enabled.is_none_or(|enabled| enabled.0))
            .flat_map(|(library, ..)| library.loaded_model_animation_set_asset_handles.iter())
            .find_map(|handle| {
                let key = &animation_clip_request.animation_clip_asset_key;
                let set = animation_set_assets.get(handle)?;
                if !set.animation_clip_asset_load_is_pending(key)
                    && set.find_animation_clip_record_by_asset_key(key).is_none()
                    && !set.animation_clip_asset_load_has_failed(key)
                    && set
                        .find_animation_clip_source_reference_by_asset_key(key)
                        .is_some()
                {
                    animation_set_assets
                        .get_mut(handle)?
                        .request_animation_clip_asset_load(&asset_server, key);
                }
                animation_set_assets
                    .get(handle)?
                    .pending_animation_clip_source_asset_handles
                    .iter()
                    .find(|(pending_key, _)| pending_key == key)
                    .map(|(_, source)| source.clone())
            });
        if let Some(source) = loading_source {
            commands
                .entity(subject)
                .try_insert(PendingAnimationClipPlayback {
                    request: request.clone(),
                    request_id,
                    source,
                    binder,
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
    use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

    #[test]
    fn missing_presentation_rejects_the_exact_requested_clip() {
        let mut application = App::new();
        application
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<AnimationSetAsset>()
            .init_asset::<AnimationClipSourceAsset>()
            .add_message::<AnimationClipPlaybackRequest>()
            .add_message::<AnimationClipPresentationSelection>()
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
