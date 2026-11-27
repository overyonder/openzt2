use bevy::prelude::*;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

use super::managed_clip_binding::ManagedAnimationClip;

#[derive(Component, PartialEq, Eq)]
pub(super) struct BoundRootMotionJoints(Box<[Entity]>);

pub(super) fn bind_root_motion_joints(
    mut commands: Commands,
    mut controllers: Query<(
        Entity,
        Ref<ManagedAnimationClip>,
        Ref<AnimationRigJoints>,
        Option<&mut BoundRootMotionJoints>,
    )>,
    joints: Query<&AnimationJointTarget>,
    changed_joints: Query<&AnimationJointTarget, Changed<AnimationJointTarget>>,
) {
    let changed_controllers = changed_joints
        .iter()
        .map(|joint| joint.animation_playback_controller_entity)
        .collect::<bevy::ecs::entity::EntityHashSet>();
    for (entity, clip, rig, existing) in &mut controllers {
        if existing.is_some()
            && !clip.is_changed()
            && !rig.is_changed()
            && !changed_controllers.contains(&entity)
        {
            continue;
        }
        let Some(root) = clip.root_motion.as_ref() else {
            if existing.is_some() {
                commands
                    .entity(entity)
                    .try_remove::<BoundRootMotionJoints>();
            }
            continue;
        };
        let resolved = BoundRootMotionJoints(
            rig.joints()
                .iter()
                .copied()
                .filter(|&joint| {
                    joints
                        .get(joint)
                        .is_ok_and(|binding| binding.authored_joint_id == root.joint)
                })
                .collect(),
        );
        if let Some(mut existing) = existing {
            existing.set_if_neq(resolved);
        } else {
            commands.entity(entity).try_insert(resolved);
        }
    }
}

pub(super) fn remove_consumed_root_trajectory(
    controllers: Query<(
        &AnimationPlaybackController,
        &ConsumedLocomotionRootMotion,
        &ManagedAnimationClip,
        &BoundRootMotionJoints,
    )>,
    mut joints: Query<&mut Transform>,
) {
    for (controller, consumed, binding, rig) in &controllers {
        let Some(root) = binding
            .root_motion
            .as_ref()
            .filter(|_| consumed.owns_playback(controller))
        else {
            continue;
        };
        let mut rig_joints = joints.iter_many_mut(rig.0.iter().copied()).matched();
        while let Some(mut transform) = rig_joints.fetch_next() {
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
