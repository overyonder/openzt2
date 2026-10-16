mod animation_graph_blend_timer_advancement;
mod animation_graph_clip_request_execution;
mod animation_graph_node_request_execution;
mod animation_graph_transition_completion;
mod embedded_gltf_animation_asset_resolution;
mod model_animation_asset_attachment;

use bevy::prelude::*;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use crate::animation_graph::animation_graph_blend_timer_advancement::advance_animation_graph_blend_timers;
use crate::animation_graph::animation_graph_clip_request_execution::execute_explicit_animation_clip_playback_requests;
use crate::animation_graph::animation_graph_node_request_execution::execute_animation_graph_node_playback_requests;
use crate::animation_graph::animation_graph_transition_completion::complete_animation_graph_transitions_after_animation_clip_completion;
use crate::animation_graph::embedded_gltf_animation_asset_resolution::resolve_loaded_gltf_animations_into_animation_set_assets;
use crate::animation_graph::model_animation_asset_attachment::attach_loaded_animation_set_assets_to_prefab_model_entities;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequestRejected;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::EmbeddedModelAnimationSetAssetHandlesByGltfAsset;

pub struct AnimationGraphPlugin;

impl Plugin for AnimationGraphPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_resource::<EmbeddedModelAnimationSetAssetHandlesByGltfAsset>()
            .add_message::<AnimationGraphNodePlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequest>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::CancelAnimationClipPlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequestRejected>()
            .add_message::<openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted>()
            .add_message::<AnimationGraphNodePlaybackRequestRejected>()
            .add_systems(
                Update,
                (
                    resolve_loaded_gltf_animations_into_animation_set_assets,
                    attach_loaded_animation_set_assets_to_prefab_model_entities,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                FixedUpdate,
                (
                    execute_animation_graph_node_playback_requests,
                    execute_explicit_animation_clip_playback_requests,
                )
                    .chain()
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                advance_animation_graph_blend_timers.in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                complete_animation_graph_transitions_after_animation_clip_completion
                    .in_set(FixedGameSet::Cleanup),
            );
    }
}
