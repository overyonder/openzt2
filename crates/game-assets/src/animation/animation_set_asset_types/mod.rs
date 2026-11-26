use std::{collections::BTreeMap, sync::Arc};

pub mod animation_set_asset_operations;

use bevy::{asset::VisitAssetDependencies, platform::collections::HashMap, prelude::*};
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_set::{
        AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata, AuthoredAnimationGraphNode,
        AuthoredAnimationSetClipReference,
    },
    animation_text_key::AuthoredAnimationTextAction,
};

#[derive(Asset, TypePath, Clone, Debug)]
pub struct AnimationSetAsset {
    pub model_asset_path: String,
    pub skeleton_asset_path: String,
    #[dependency]
    pub(crate) animation_clips: Vec<AnimationClipRecord>,
    pub(crate) embedded_clip_indices: HashMap<String, usize>,
    #[dependency]
    pub embedded_animation_graph: Option<Handle<AnimationGraph>>,
    pub animation_graph_nodes: Vec<AuthoredAnimationGraphNode>,
    pub animation_graph_order: Vec<String>,
    pub animation_graph_edges: Vec<AuthoredAnimationGraphEdge>,
    graph_node_indices: HashMap<String, usize>,
    outgoing_edges: Vec<Vec<(u32, usize)>>,
    pub animation_graph_metadata: Option<AuthoredAnimationGraphMetadata>,
    pub(crate) animation_clip_source_references: Vec<AuthoredAnimationSetClipReference>,
    pub(crate) clip_source_reference_indices: HashMap<String, usize>,
    #[dependency]
    pub(crate) preloaded_clip_sources: BTreeMap<String, Handle<AnimationClipSourceAsset>>,
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub struct AnimationClipRecord {
    pub animation_clip_asset_key: String,
    #[dependency]
    pub animation_clip_handle: Handle<AnimationClip>,
    pub duration_milliseconds: u32,
    pub(crate) graph_node: Option<AnimationNodeIndex>,
}

#[derive(Asset, TypePath, Clone, Debug)]
pub struct AnimationClipSourceAsset {
    pub authored_animation_clip_metadata: Arc<AuthoredAnimationClipMetadata>,
    pub timeline: Arc<[AnimationTimelineKey]>,
    #[dependency]
    pub animation_clip_handle: Handle<AnimationClip>,
    #[dependency]
    pub animation_graph: Handle<AnimationGraph>,
    pub animation_node: AnimationNodeIndex,
    pub loop_bounds: Option<std::ops::Range<f32>>,
}

#[derive(EntityEvent, Clone)]
pub struct AuthoredAnimationKeyframeEvent {
    pub entity: Entity,
    pub timeline: Arc<[AnimationTimelineKey]>,
    pub first_key: usize,
    pub end_key: usize,
}

#[derive(Debug)]
pub struct AnimationTimelineKey {
    pub milliseconds: u32,
    pub commands: Box<[AnimationTimelineCommand]>,
}
#[derive(Debug)]
pub struct AnimationTimelineCommand {
    pub joint: Option<openzt2_game_data::AssetId>,
    pub action: Arc<AuthoredAnimationTextAction>,
}
