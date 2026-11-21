use bevy::{
    animation::{animated_field, AnimationTargetId},
    prelude::*,
};
use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};

/// Actor manifests attached to the physical binder that owns the paths.
#[derive(Component, PartialEq, Eq)]
pub struct PrefabTraversalAnimationManifests(pub Box<[Handle<AnimationSetAsset>]>);

#[derive(Component)]
pub struct TraversalAnimationPath {
    pub clip_key: String,
    pub source: Handle<AnimationClipSourceAsset>,
}

impl TraversalAnimationPath {
    pub fn local_pose(
        &self,
        progress: f32,
        sources: &Assets<AnimationClipSourceAsset>,
        clips: &Assets<AnimationClip>,
    ) -> Option<Transform> {
        let source = sources.get(&self.source)?;
        let root = source
            .authored_animation_clip_metadata
            .root_motion_joint
            .as_ref()?;
        let clip = clips.get(&source.animation_clip_handle)?;
        let key = root.joint_asset_key.to_ascii_lowercase();
        let target = AnimationTargetId::from_iter([key.as_str()]);
        let time = progress.clamp(0.0, 1.0) * clip.duration();
        let position =
            clip.sample_clamped(animated_field!(Transform::translation), target, time)?;
        let rotation = clip
            .sample_clamped(animated_field!(Transform::rotation), target, time)
            .unwrap_or(Quat::IDENTITY);
        Some(Transform::from_translation(position).with_rotation(rotation))
    }
}
