use super::{AnimationClipRecord, AnimationSetAsset};
use bevy::prelude::*;
use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata, AuthoredAnimationGraphNode,
    AuthoredAnimationPlaybackPolicy, AuthoredAnimationSetClipReference,
};

impl AnimationSetAsset {
    pub(crate) fn create_ready_animation_set_asset(
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
            animation_graph_nodes,
            animation_graph_order,
            animation_graph_edges,
            animation_graph_metadata,
            animation_clip_source_references: Vec::new(),
            pending_animation_clip_source_asset_handles: Vec::new(),
            failed_animation_clip_asset_keys: Vec::new(),
        }
    }

    pub fn find_animation_clip_record_by_asset_key(
        &self,
        animation_clip_asset_key: &str,
    ) -> Option<&AnimationClipRecord> {
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

    pub(crate) fn find_animation_clip_source_reference_by_asset_key(
        &self,
        animation_clip_asset_key: &str,
    ) -> Option<&AuthoredAnimationSetClipReference> {
        self.animation_clip_source_references
            .iter()
            .find(|animation_clip_source_reference| {
                animation_clip_source_reference.animation_clip_asset_key == animation_clip_asset_key
            })
    }

    pub(crate) fn request_animation_clip_asset_load(
        &mut self,
        asset_server: &AssetServer,
        animation_clip_asset_key: &str,
    ) -> bool {
        if self.animation_clip_asset_load_has_failed(animation_clip_asset_key) {
            return false;
        }
        if self
            .find_animation_clip_record_by_asset_key(animation_clip_asset_key)
            .is_some()
            || self.pending_animation_clip_source_asset_handles.iter().any(
                |(pending_animation_clip_asset_key, _)| {
                    pending_animation_clip_asset_key == animation_clip_asset_key
                },
            )
        {
            return true;
        }
        let Some(animation_clip_source_reference) =
            self.find_animation_clip_source_reference_by_asset_key(animation_clip_asset_key)
        else {
            self.failed_animation_clip_asset_keys
                .push(animation_clip_asset_key.to_owned());
            return false;
        };
        self.pending_animation_clip_source_asset_handles.push((
            animation_clip_asset_key.to_owned(),
            asset_server.load(
                animation_clip_source_reference
                    .animation_clip_asset_path
                    .clone(),
            ),
        ));
        true
    }

    pub(crate) fn animation_clip_asset_load_is_pending(
        &self,
        animation_clip_asset_key: &str,
    ) -> bool {
        self.pending_animation_clip_source_asset_handles.iter().any(
            |(pending_animation_clip_asset_key, _)| {
                pending_animation_clip_asset_key == animation_clip_asset_key
            },
        )
    }

    pub(crate) fn animation_clip_asset_load_has_failed(
        &self,
        animation_clip_asset_key: &str,
    ) -> bool {
        self.failed_animation_clip_asset_keys
            .iter()
            .any(|failed_animation_clip_asset_key| {
                failed_animation_clip_asset_key == animation_clip_asset_key
            })
    }
}

impl AnimationClipRecord {
    pub(crate) fn create_embedded_animation_clip_record(
        animation_clip_asset_key: String,
        animation_clip_handle: Handle<AnimationClip>,
        animation_duration_milliseconds: u32,
    ) -> Self {
        Self {
            animation_clip_asset_key,
            animation_clip_handle,
            duration_milliseconds: animation_duration_milliseconds,
            authored_animation_clip_metadata: None,
            authored_animation_policy: AuthoredAnimationPlaybackPolicy::default(),
        }
    }

    pub(crate) fn authored_playback_rate(&self) -> f32 {
        self.authored_animation_policy.playback_rate.unwrap_or(1.0)
    }

    pub(crate) fn authored_playback_is_looped(&self) -> bool {
        self.authored_animation_clip_metadata
            .as_ref()
            .is_some_and(|animation_clip_metadata| animation_clip_metadata.playback_is_looped)
    }
}
