use std::sync::Arc;

mod animation_set_asset_operations;

use bevy::prelude::*;
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_set::{
        AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata, AuthoredAnimationGraphNode,
        AuthoredAnimationPlaybackPolicy, AuthoredAnimationSetClipReference,
    },
};

#[derive(Asset, TypePath, Clone, Debug)]
pub struct AnimationSetAsset {
    pub model_asset_path: String,
    pub skeleton_asset_path: String,
    pub animation_clips: Vec<AnimationClipRecord>,
    pub animation_graph_nodes: Vec<AuthoredAnimationGraphNode>,
    pub animation_graph_order: Vec<String>,
    pub animation_graph_edges: Vec<AuthoredAnimationGraphEdge>,
    pub animation_graph_metadata: Option<AuthoredAnimationGraphMetadata>,
    pub(crate) animation_clip_source_references: Vec<AuthoredAnimationSetClipReference>,
    pub(crate) pending_animation_clip_source_asset_handles:
        Vec<(String, Handle<AnimationClipSourceAsset>)>,
    pub(crate) failed_animation_clip_asset_keys: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AnimationClipRecord {
    pub animation_clip_asset_key: String,
    pub animation_clip_handle: Handle<AnimationClip>,
    pub duration_milliseconds: u32,
    pub authored_animation_clip_metadata: Option<Arc<AuthoredAnimationClipMetadata>>,
    pub authored_animation_policy: AuthoredAnimationPlaybackPolicy,
}

#[derive(Asset, TypePath, Clone, Debug)]
pub(crate) struct AnimationClipSourceAsset {
    pub(crate) authored_animation_clip_metadata: Arc<AuthoredAnimationClipMetadata>,
    pub(crate) animation_clip_handle: Handle<AnimationClip>,
}
