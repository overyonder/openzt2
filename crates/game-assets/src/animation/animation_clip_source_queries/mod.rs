use bevy::{
    asset::{AssetPath, LoadState},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_set::AuthoredAnimationPlaybackPolicy,
};

use super::animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset};

#[derive(SystemParam)]
pub struct AnimationClipSources<'w> {
    server: Res<'w, AssetServer>,
    sources: Res<'w, Assets<AnimationClipSourceAsset>>,
}

pub struct LoadedAnimationClip<'a> {
    pub clip: &'a Handle<AnimationClip>,
    pub duration_milliseconds: u32,
    pub metadata: Option<&'a AuthoredAnimationClipMetadata>,
    pub policy: Option<&'a AuthoredAnimationPlaybackPolicy>,
    pub graph: Option<(&'a Handle<AnimationGraph>, AnimationNodeIndex)>,
}

impl AnimationClipSources<'_> {
    pub fn retained_handle(
        &self,
        set: &AnimationSetAsset,
        key: &str,
    ) -> Option<Handle<AnimationClipSourceAsset>> {
        if let Some(preloaded) = set.preloaded_clip_sources.get(key) {
            return Some(preloaded.clone());
        }
        let reference = set.find_animation_clip_source_reference_by_asset_key(key)?;
        self.server
            .get_handle(AssetPath::parse(&reference.animation_clip_asset_path))
    }

    pub fn request(
        &self,
        set: &AnimationSetAsset,
        key: &str,
    ) -> Option<Handle<AnimationClipSourceAsset>> {
        self.retained_handle(set, key).or_else(|| {
            let reference = set.find_animation_clip_source_reference_by_asset_key(key)?;
            Some(
                self.server
                    .load(reference.animation_clip_asset_path.clone()),
            )
        })
    }

    pub fn failed(&self, handle: &Handle<AnimationClipSourceAsset>) -> bool {
        matches!(self.server.load_state(handle.id()), LoadState::Failed(_))
    }

    pub fn resolved(&self, handle: &Handle<AnimationClipSourceAsset>) -> bool {
        self.sources.contains(handle.id())
    }

    pub fn get<'a>(
        &'a self,
        set: &'a AnimationSetAsset,
        key: &str,
    ) -> Option<LoadedAnimationClip<'a>> {
        if let Some(record) = set.embedded_clip(key) {
            return Some(LoadedAnimationClip {
                clip: &record.animation_clip_handle,
                duration_milliseconds: record.duration_milliseconds,
                metadata: None,
                policy: None,
                graph: set.embedded_animation_graph.as_ref().and_then(|graph| {
                    let index = set
                        .animation_clips
                        .iter()
                        .position(|clip| clip.animation_clip_asset_key == key)?;
                    Some((graph, AnimationNodeIndex::new(index + 1)))
                }),
            });
        }
        let reference = set.find_animation_clip_source_reference_by_asset_key(key)?;
        let handle = self.retained_handle(set, key)?;
        let source = self.sources.get(handle.id())?;
        Some(LoadedAnimationClip {
            clip: &source.animation_clip_handle,
            duration_milliseconds: source
                .authored_animation_clip_metadata
                .duration_milliseconds
                .max(1),
            metadata: Some(&source.authored_animation_clip_metadata),
            policy: Some(&reference.playback_policy),
            graph: Some((&source.animation_graph, source.animation_node)),
        })
    }
}

impl LoadedAnimationClip<'_> {
    pub fn authored_playback_rate(&self) -> f32 {
        self.policy
            .and_then(|policy| policy.playback_rate)
            .unwrap_or(1.0)
    }

    pub fn authored_playback_is_looped(&self) -> bool {
        self.metadata
            .is_some_and(|metadata| metadata.playback_is_looped)
    }
}
