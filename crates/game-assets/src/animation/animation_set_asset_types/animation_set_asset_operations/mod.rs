use super::{AnimationClipRecord, AnimationSetAsset};
use bevy::prelude::*;
use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata, AuthoredAnimationGraphNode,
    AuthoredAnimationSetClipReference,
};

impl AnimationSetAsset {
    pub fn create_ready_animation_set_asset(
        model_asset_path: String,
        skeleton_asset_path: String,
        animation_clips: Vec<AnimationClipRecord>,
        animation_graph_nodes: Vec<AuthoredAnimationGraphNode>,
        animation_graph_order: Vec<String>,
        animation_graph_edges: Vec<AuthoredAnimationGraphEdge>,
        animation_graph_metadata: Option<AuthoredAnimationGraphMetadata>,
    ) -> Self {
        Self {
            model_asset_path,
            skeleton_asset_path,
            animation_clips,
            embedded_animation_graph: None,
            animation_graph_nodes,
            animation_graph_order,
            animation_graph_edges,
            animation_graph_metadata,
            animation_clip_source_references: Vec::new(),
            preloaded_clip_sources: Default::default(),
        }
    }

    pub fn embedded_clip(&self, animation_clip_asset_key: &str) -> Option<&AnimationClipRecord> {
        self.animation_clips.iter().find(|animation_clip| {
            animation_clip.animation_clip_asset_key == animation_clip_asset_key
        })
    }

    pub fn find_animation_graph_node_index_by_asset_key(
        &self,
        animation_graph_node_asset_key: &str,
    ) -> Option<usize> {
        self.animation_graph_nodes
            .iter()
            .position(|animation_graph_node| {
                animation_graph_node.animation_graph_node_asset_key
                    == animation_graph_node_asset_key
            })
    }

    pub fn find_animation_clip_source_reference_by_asset_key(
        &self,
        animation_clip_asset_key: &str,
    ) -> Option<&AuthoredAnimationSetClipReference> {
        self.animation_clip_source_references
            .iter()
            .find(|animation_clip_source_reference| {
                animation_clip_source_reference.animation_clip_asset_key == animation_clip_asset_key
            })
    }
}

impl AnimationClipRecord {
    pub fn create_embedded_animation_clip_record(
        animation_clip_asset_key: String,
        animation_clip_handle: Handle<AnimationClip>,
        animation_duration_milliseconds: u32,
    ) -> Self {
        Self {
            animation_clip_asset_key,
            animation_clip_handle,
            duration_milliseconds: animation_duration_milliseconds,
        }
    }
}
