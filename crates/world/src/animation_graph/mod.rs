mod animation_graph_blend_timer_advancement;
mod animation_graph_clip_request_execution;
mod animation_graph_node_request_execution;
mod animation_graph_route_selection;
mod animation_graph_transition_completion;
mod animation_return_route;
mod embedded_model_animation_binding;
mod model_animation_asset_attachment;

use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::model_render::ModelExpanded;

use openzt2_world_state::animation_playback::locomotion_root_motion::AnimationLocomotionMotionSampling;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use crate::animation_graph::animation_graph_blend_timer_advancement::{
    advance_animation_graph_blend_timers, select_advancing_animation_graph_blends,
};
use crate::animation_graph::animation_graph_clip_request_execution::execute_explicit_animation_clip_playback_requests;
use crate::animation_graph::animation_graph_node_request_execution::execute_animation_graph_node_playback_requests;
use crate::animation_graph::animation_graph_transition_completion::complete_animation_graph_transitions_after_animation_clip_completion;
use crate::animation_graph::model_animation_asset_attachment::attach_loaded_animation_set_assets_to_animation_subject_entities;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipActionOptions;
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
            .add_message::<AnimationClipActionOptions>()
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
                FixedUpdate,
                (
                    execute_animation_graph_node_playback_requests
                        .run_if(on_message::<AnimationGraphNodePlaybackRequest>),
                    execute_explicit_animation_clip_playback_requests,
                    select_advancing_animation_graph_blends,
                    advance_animation_graph_blend_timers,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .before(AnimationLocomotionMotionSampling),
            )
            .add_systems(
                FixedUpdate,
                (
                    animation_return_route::receive_return_route_requests,
                    animation_return_route::advance_return_routes,
                    complete_animation_graph_transitions_after_animation_clip_completion,
                ).chain().in_set(FixedGameSet::Cleanup),
            );
    }
}
