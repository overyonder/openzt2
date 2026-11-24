use bevy::prelude::*;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

use super::managed_clip_binding::ManagedAnimationClip;

pub(super) fn remove_consumed_root_trajectory(
    controllers: Query<(
        &AnimationPlaybackController,
        &ConsumedLocomotionRootMotion,
        &ManagedAnimationClip,
        &AnimationRigJoints,
    )>,
    mut joints: Query<(&AnimationJointTarget, &mut Transform)>,
) {
    for (controller, consumed, binding, rig) in &controllers {
        let Some(root) = binding
            .root_motion
            .as_ref()
            .filter(|_| consumed.owns_playback(controller))
        else {
            continue;
        };
        let mut rig_joints = joints.iter_many_mut(rig.joints()).matched();
        while let Some((binding, mut transform)) = rig_joints.fetch_next() {
            if binding.authored_joint_id != root.joint {
                continue;
            }
            let mut pose = *transform;
            if let Some(translation) = root.entry_translation {
                pose.translation = translation;
            }
            if let Some(rotation) = root.entry_rotation {
                pose.rotation = rotation;
            }
            transform.set_if_neq(pose);
        }
    }
}
