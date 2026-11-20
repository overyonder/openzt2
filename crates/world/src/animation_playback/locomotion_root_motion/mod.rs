use bevy::animation::{animated_field, AnimationTargetId};
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

pub(crate) fn remove_consumed_root_trajectory(
    assets: Res<Assets<AnimationSetAsset>>,
    clips: Res<Assets<AnimationClip>>,
    controllers: Query<(
        &AnimationPlaybackController,
        &ConsumedLocomotionRootMotion,
        &AnimationRigJoints,
    )>,
    mut joints: Query<(&AnimationJointTarget, &mut Transform)>,
) {
    for (controller, consumed, rig) in &controllers {
        if !consumed.owns_playback(controller) {
            continue;
        }
        let Some(record) = assets.get(&controller.animation_set_asset).and_then(|set| {
            set.find_animation_clip_record_by_asset_key(&controller.animation_clip_asset_key)
        }) else {
            continue;
        };
        let Some(root) = record
            .authored_animation_clip_metadata
            .as_ref()
            .and_then(|metadata| metadata.root_motion_joint.as_ref())
        else {
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
