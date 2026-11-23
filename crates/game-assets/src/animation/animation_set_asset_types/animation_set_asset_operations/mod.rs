use super::{AnimationClipRecord, AnimationSetAsset};
use bevy::{platform::collections::HashMap, prelude::*};
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
        let mut embedded_clip_indices = HashMap::with_capacity(animation_clips.len());
        for (index, clip) in animation_clips.iter().enumerate() {
            embedded_clip_indices
                .entry(clip.animation_clip_asset_key.clone())
                .or_insert(index);
        }
        Self {
            model_asset_path,
            skeleton_asset_path,
            animation_clips,
            embedded_clip_indices,
            embedded_animation_graph: None,
            animation_graph_nodes,
            animation_graph_order,
            animation_graph_edges,
            animation_graph_metadata,
            animation_clip_source_references: Vec::new(),
            clip_source_reference_indices: HashMap::default(),
            preloaded_clip_sources: Default::default(),
        }
    }

    pub fn animation_clips(&self) -> &[AnimationClipRecord] {
        &self.animation_clips
    }

    pub fn animation_clip_source_references(&self) -> &[AuthoredAnimationSetClipReference] {
        &self.animation_clip_source_references
    }

    pub fn set_animation_clip_source_references(
        &mut self,
        references: Vec<AuthoredAnimationSetClipReference>,
    ) {
        self.preloaded_clip_sources.clear();
        self.clip_source_reference_indices.clear();
        self.clip_source_reference_indices.reserve(references.len());
        for (index, reference) in references.iter().enumerate() {
            self.clip_source_reference_indices
                .entry(reference.animation_clip_asset_key.clone())
                .or_insert(index);
        }
        self.animation_clip_source_references = references;
    }

    pub fn embedded_clip(&self, key: &str) -> Option<&AnimationClipRecord> {
        let &index = self.embedded_clip_indices.get(key)?;
        self.animation_clips.get(index)
    }

    pub fn find_animation_clip_source_reference_by_asset_key(
        &self,
        key: &str,
    ) -> Option<&AuthoredAnimationSetClipReference> {
        let &index = self.clip_source_reference_indices.get(key)?;
        self.animation_clip_source_references.get(index)
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
            graph_node: None,
        }
    }
}
