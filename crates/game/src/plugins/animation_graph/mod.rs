mod animation_graph_blend_timer_advancement;
mod animation_graph_clip_request_execution;
mod animation_graph_node_request_execution;
pub(crate) mod animation_graph_playback_message_types;
pub(crate) mod animation_graph_playback_state_types;
mod animation_graph_transition_completion;
pub(crate) mod animation_graph_transition_operations;
pub(crate) mod animation_presentation_relationship_types;
mod authored_animation_graph_edge_blend_duration_interpretation;
mod embedded_gltf_animation_asset_resolution;
mod model_animation_asset_attachment;
pub(crate) mod model_animation_asset_binding_types;

use bevy::prelude::*;

use crate::application_schedule::{FixedGameSet, GameSet};

use animation_graph_blend_timer_advancement::advance_animation_graph_blend_timers;
use animation_graph_clip_request_execution::execute_explicit_animation_clip_playback_requests;
use animation_graph_node_request_execution::execute_animation_graph_node_playback_requests;
use animation_graph_playback_message_types::{
    AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
    AnimationGraphNodePlaybackRequest, AnimationGraphNodePlaybackRequestRejected,
};
use animation_graph_transition_completion::complete_animation_graph_transitions_after_animation_clip_completion;
use embedded_gltf_animation_asset_resolution::resolve_loaded_gltf_animations_into_animation_set_assets;
use model_animation_asset_attachment::attach_loaded_animation_set_assets_to_prefab_model_entities;
use model_animation_asset_binding_types::EmbeddedModelAnimationSetAssetHandlesByGltfAsset;

pub(crate) struct AnimationGraphPlugin;

impl Plugin for AnimationGraphPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_resource::<EmbeddedModelAnimationSetAssetHandlesByGltfAsset>()
            .add_message::<AnimationGraphNodePlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequest>()
            .add_message::<AnimationClipPlaybackRequestRejected>()
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
