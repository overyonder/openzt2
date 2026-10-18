use bevy::prelude::*;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

use super::standard_bevy_animation_player_projection_types::StandardBevyAnimationJointBlendPose;

pub(super) fn blend_sampled_animation_joint_poses(
    mut commands: Commands,
    controllers: Query<(&AnimationPlaybackController, &AnimationGraphPlaybackState)>,
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
            .and_then(|(_, graph)| graph.active_animation_graph_blend)
            .filter(|blend| blend.duration_milliseconds > 0);
        let Some(blend) = blend else {
            commands
                .entity(entity)
                .remove::<StandardBevyAnimationJointBlendPose>();
            continue;
        };
        // The simulation graph owns blend time. Bevy samples the incoming clip
        // once, and this rendering snapshot supplies the outgoing pose.
        let fraction = (std::time::Duration::from_millis(u64::from(blend.elapsed_milliseconds))
            .as_secs_f32()
            / std::time::Duration::from_millis(u64::from(blend.duration_milliseconds))
                .as_secs_f32())
        .clamp(0.0, 1.0);
        transform.translation = snapshot
            .pose
            .translation
            .lerp(transform.translation, fraction);
        transform.rotation = snapshot.pose.rotation.slerp(transform.rotation, fraction);
        transform.scale = snapshot.pose.scale.lerp(transform.scale, fraction);
    }
}
