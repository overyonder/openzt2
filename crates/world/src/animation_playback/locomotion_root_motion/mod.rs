use bevy::animation::{animated_field, AnimationTargetId};
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationClipSourceAsset;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::{
    AnimationPlaybackController, RetainedAnimationClipSource,
};
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

pub(crate) fn remove_consumed_root_trajectory(
    sources: Res<Assets<AnimationClipSourceAsset>>,
    clips: Res<Assets<AnimationClip>>,
    controllers: Query<(
        &AnimationPlaybackController,
        &ConsumedLocomotionRootMotion,
        &RetainedAnimationClipSource,
        &AnimationRigJoints,
    )>,
    mut joints: Query<(&AnimationJointTarget, &mut Transform)>,
) {
    for (controller, consumed, retained, rig) in &controllers {
        if !consumed.owns_playback(controller) {
            continue;
        }
        let Some(record) = sources.get(&retained.0) else {
            continue;
        };
        let Some(root) = &record.authored_animation_clip_metadata.root_motion_joint else {
            continue;
        };
        let Some(clip) = clips.get(&record.animation_clip_handle) else {
            continue;
        };
        let target =
            AnimationTargetId::from_name(&Name::new(root.joint_asset_key.to_ascii_lowercase()));
        // Navigation has consumed the sampled displacement and rotation. Every
        // skin retains the same clip-entry root pose, including after wrapping.
        let translation = clip.sample_clamped(animated_field!(Transform::translation), target, 0.0);
        let rotation = clip.sample_clamped(animated_field!(Transform::rotation), target, 0.0);
        let mut rig_joints = joints.iter_many_mut(rig.joints()).matched();
        while let Some((binding, mut transform)) = rig_joints.fetch_next() {
            if !binding
                .joint_asset_key
                .eq_ignore_ascii_case(&root.joint_asset_key)
            {
                continue;
            }
            if let Some(translation) = translation {
                transform.translation = translation;
            }
            if let Some(rotation) = rotation {
                transform.rotation = rotation;
            }
        }
    }
}
