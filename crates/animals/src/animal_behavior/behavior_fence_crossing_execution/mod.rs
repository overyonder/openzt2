//! A fence action moves its actor along the authored clip's root trajectory.

use bevy::prelude::*;
use openzt2_game_assets::{
    animation::animation_set_asset_types::AnimationClipSourceAsset,
    application_lifecycle::GamePhase,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    animation_playback::{
        animation_playback_controller_types::{
            AnimationPlaybackController, RetainedAnimationClipSource,
        },
        locomotion_root_motion::{AnimationLocomotionMotionSampling, ConsumedLocomotionRootMotion},
    },
    application_schedule::FixedGameSet,
};
use openzt2_zoo_state::{
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{
        running_actions::{RunBehaviorAnimationClip, RunningBehaviorInstruction},
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
        PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{Destination, Docking},
    topology::topology_graph_types::FenceEdge,
};

use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};

#[derive(Component)]
struct FenceAnimationMotion {
    animal: Entity,
    request: usize,
}

pub(super) fn register_fence_crossing_execution(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            bind_fence_action_root_motion.before(AnimationLocomotionMotionSampling),
            apply_fence_action_root_motion
                .after(AnimationLocomotionMotionSampling)
                .before(AnimationObjectCommandDelivery),
        )
            .in_set(FixedGameSet::Act)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn bind_fence_action_root_motion(
    controllers: Query<(
        Entity,
        &AnimationPlaybackController,
        &RetainedAnimationClipSource,
        &AnimationRigJoints,
        Option<&AnimationPresentationOwner>,
        Option<&FenceAnimationMotion>,
        Option<&ConsumedLocomotionRootMotion>,
    )>,
    animals: Query<
        (
            &GlobalTransform,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &PendingBehaviorAnimationClipCompletion,
        ),
        (
            With<Animal>,
            With<RunBehaviorAnimationClip>,
            Without<PendingBehaviorTaskFailure>,
            Without<Docking>,
            Without<Destination>,
        ),
    >,
    fences: Query<(), With<FenceEdge>>,
    clips: Res<Assets<AnimationClipSourceAsset>>,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    for (controller, playback, source, rig, owner, current, motion) in &controllers {
        let animal = owner.map_or(controller, |owner| owner.gameplay_entity);
        let active = animals
            .get(animal)
            .ok()
            .filter(|(_, task, instruction, pending)| {
                task.target().is_some_and(|fence| fences.contains(fence))
                    && pending.owns(task, instruction)
                    && playback.explicit_clip_request_id == Some(pending.request_id)
            });
        if current.is_some_and(|current| {
            active.is_some_and(|(_, _, _, pending)| current.request == pending.request_id)
        }) {
            continue;
        }
        if let Some(current) = current {
            commands.entity(controller).remove::<FenceAnimationMotion>();
            if playback.explicit_clip_request_id == Some(current.request)
                && motion.is_some_and(|motion| motion.owns_playback(playback))
            {
                commands
                    .entity(controller)
                    .remove::<ConsumedLocomotionRootMotion>();
            }
        }
        let Some((actor_pose, _, _, pending)) = active else {
            continue;
        };
        let Some(joint) = clips.get(&source.0).and_then(|clip| {
            clip.authored_animation_clip_metadata
                .root_motion_joint
                .as_ref()
        }) else {
            continue;
        };
        let root = AssetId::from_key(&joint.joint_asset_key);
        let Some(parent) =
            joints
                .iter_many(rig.joints())
                .flatten()
                .find_map(|(binding, parent)| {
                    (binding.authored_joint_id == root)
                        .then(|| transforms.get(parent.parent()).ok())
                        .flatten()
                })
        else {
            continue;
        };
        commands.entity(controller).insert((
            FenceAnimationMotion {
                animal,
                request: pending.request_id,
            },
            ConsumedLocomotionRootMotion::new(
                pending.request_id,
                actor_pose.affine().inverse() * parent.affine(),
            ),
        ));
    }
}

fn apply_fence_action_root_motion(
    motions: Query<(
        &FenceAnimationMotion,
        &ConsumedLocomotionRootMotion,
        &AnimationPlaybackController,
    )>,
    mut animals: Query<
        (
            &mut Transform,
            Option<&mut avian3d::prelude::Position>,
            Option<&mut avian3d::prelude::Rotation>,
        ),
        With<Animal>,
    >,
) {
    for (owner, motion, playback) in &motions {
        if !motion.owns_playback(playback)
            || playback.explicit_clip_request_id != Some(owner.request)
        {
            continue;
        }
        let Ok((mut pose, position, rotation)) = animals.get_mut(owner.animal) else {
            continue;
        };
        let delta = pose.rotation * (pose.scale * motion.translation());
        let turn = motion.rotation();
        if !delta.is_finite()
            || !turn.is_finite()
            || (delta == Vec3::ZERO && turn == Quat::IDENTITY)
        {
            continue;
        }
        pose.translation += delta;
        pose.rotation = (pose.rotation * turn).normalize();
        if let Some(mut position) = position {
            position.0 = pose.translation;
        }
        if let Some(mut rotation) = rotation {
            rotation.0 = pose.rotation;
        }
    }
}
