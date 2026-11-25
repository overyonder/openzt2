use crate::animation_graph::animation_graph_blend_timer_advancement::FrozenAnimationPoseBlend;
use bevy::prelude::*;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

use super::standard_bevy_animation_player_projection_types::StandardBevyAnimationJointBlendPose;

pub(super) fn blend_sampled_animation_joint_poses(
    mut commands: Commands,
    controllers: Query<(&AnimationPlaybackController, &FrozenAnimationPoseBlend)>,
    mut joints: Query<(
        Entity,
        &AnimationJointTarget,
        &StandardBevyAnimationJointBlendPose,
        &mut Transform,
    )>,
) {
    for (entity, binding, snapshot, mut transform) in &mut joints {
        let blend = controllers
            .get(binding.animation_playback_controller_entity)
            .ok()
            .filter(|(controller, _)| {
                controller.playback_generation == snapshot.playback_generation
            })
            .map(|(_, blend)| blend);
        let Some(blend) = blend else {
            commands
                .entity(entity)
                .remove::<StandardBevyAnimationJointBlendPose>();
            continue;
        };
        // The simulation graph owns blend time. Bevy samples the incoming clip
        // once, and this rendering snapshot supplies the outgoing pose.
        let fraction = blend.0.fraction();
        transform.translation = snapshot
            .pose
            .translation
            .lerp(transform.translation, fraction);
        transform.rotation = snapshot.pose.rotation.slerp(transform.rotation, fraction);
        transform.scale = snapshot.pose.scale.lerp(transform.scale, fraction);
    }
}
