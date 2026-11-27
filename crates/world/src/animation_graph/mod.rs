use bevy::asset::AssetLoadFailedEvent;
pub(crate) mod animation_graph_blend_timer_advancement;
mod animation_graph_clip_request_execution;
mod animation_graph_node_request_execution;
pub(crate) mod animation_graph_route_selection;
mod animation_graph_transition_completion;
mod animation_return_route;
mod embedded_model_animation_binding;
pub(crate) mod model_animation_asset_attachment;

use crate::animation_playback::managed_clip_binding::{
    self, FixedAnimationPlayer, RealTimeAnimationPlayer,
};
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::model_render::ModelExpanded;

use openzt2_world_state::animation_playback::locomotion_root_motion::AnimationLocomotionMotionSampling;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use crate::animation_graph::animation_graph_clip_request_execution::execute_explicit_animation_clip_playback_requests;
use crate::animation_graph::animation_graph_node_request_execution::execute_animation_graph_node_playback_requests;
use crate::animation_graph::animation_graph_transition_completion::complete_animation_graph_transitions_after_animation_clip_completion;
use crate::animation_graph::model_animation_asset_attachment::attach_loaded_animation_set_assets_to_animation_subject_entities;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequestRejected;

pub struct AnimationGraphPlugin;

impl Plugin for AnimationGraphPlugin {
    fn build(&self, application: &mut App) {
        application
            .add_message::<AnimationGraphNodePlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequest>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationReturnRouteRequest>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequestRejected>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted>()
            .add_message::<AnimationGraphNodePlaybackRequestRejected>()
            .add_systems(
                PreUpdate,
                animation_graph_clip_request_execution::wake_animation_clip_requests_after_asset_loading
                    .after(bevy::asset::AssetTrackingSystems),
            )
            .add_systems(
                Update,
                (
                    embedded_model_animation_binding::request_embedded_model_animation_sets,
                    embedded_model_animation_binding::refresh_embedded_model_animation_bindings,
                ).before(attach_loaded_animation_set_assets_to_animation_subject_entities).in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                attach_loaded_animation_set_assets_to_animation_subject_entities
                    .in_set(GameSet::Intent)
                    .run_if(
                        any_match_filter::<(With<PendingModelAnimationAssets>, Or<(Changed<PendingModelAnimationAssets>, Added<ModelExpanded>)>)>
                            .or_eager(on_message::<AssetEvent<AnimationClipSourceAsset>>)
                            .or_eager(on_message::<AssetEvent<AnimationSetAsset>>)
                            .or_eager(on_message::<AssetLoadFailedEvent<AnimationSetAsset>>)
                            .or_eager(on_message::<AssetLoadFailedEvent<AnimationClipSourceAsset>>),
                    ),
            )
            .add_systems(
                Update,
                (
                    complete_animation_graph_transitions_after_animation_clip_completion::<RealTimeAnimationPlayer>
                        .run_if(on_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted>),
                    animation_graph_route_selection::continue_prepared_kfm_routes::<RealTimeAnimationPlayer>,
                    animation_graph_node_request_execution::resume_animation_graph_node_routes::<RealTimeAnimationPlayer>,
                    execute_animation_graph_node_playback_requests::<RealTimeAnimationPlayer, true>
                        .run_if(on_message::<AnimationGraphNodePlaybackRequest>),
                    execute_explicit_animation_clip_playback_requests::<true>,
                )
                    .chain()
                    .after(attach_loaded_animation_set_assets_to_animation_subject_entities)
                    .before(managed_clip_binding::bind_selected_clips::<RealTimeAnimationPlayer>),
            )
            .add_systems(
                FixedUpdate,
                (
                    animation_graph_route_selection::continue_prepared_kfm_routes::<FixedAnimationPlayer>,
                    animation_graph_node_request_execution::resume_animation_graph_node_routes::<FixedAnimationPlayer>,
                    execute_animation_graph_node_playback_requests::<FixedAnimationPlayer, false>
                        .run_if(on_message::<AnimationGraphNodePlaybackRequest>),
                    execute_explicit_animation_clip_playback_requests::<false>,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .after(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet::Think)
                    .before(AnimationLocomotionMotionSampling),
            )
            .add_systems(
                FixedUpdate,
                (
                    animation_return_route::receive_return_route_requests,
                    animation_return_route::advance_return_routes,
                    complete_animation_graph_transitions_after_animation_clip_completion::<FixedAnimationPlayer>,
                )
                    .chain()
                    .after(AnimationLocomotionMotionSampling)
                    .after(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
                    .in_set(FixedGameSet::Cleanup),
            );
    }
}
