use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

pub(crate) fn remove_consumed_root_trajectory(
    assets: Res<Assets<AnimationSetAsset>>,
    controllers: Query<(&AnimationPlaybackController, &ConsumedLocomotionRootMotion)>,
    mut joints: Query<(&AnimationJointTarget, &mut Transform)>,
) {
    for (binding, mut transform) in &mut joints {
        let Ok((controller, consumed)) =
            controllers.get(binding.animation_playback_controller_entity)
        else {
            continue;
        };
        if controller.explicit_clip_request_id != Some(consumed.playback_request_id) {
            continue;
        }
        let Some(clip) = assets.get(&controller.animation_set_asset).and_then(|set| {
            set.find_animation_clip_record_by_asset_key(&controller.animation_clip_asset_key)
        }) else {
            continue;
        };
        let Some(metadata) = clip.authored_animation_clip_metadata.as_ref() else {
            continue;
        };
        let (Some(root), Some(motion)) =
            (&metadata.root_motion_joint, metadata.root_motion_transform)
        else {
            continue;
        };
        if binding.joint_asset_key == root.joint_asset_key && clip.duration_milliseconds > 0 {
            // Bevy evaluates precisely this controller seek time; the controller
            // already applies authored rate and wraps. Remove only the uniform
            // trajectory consumed by navigation, preserving residual gait pose.
            let fraction = (controller.elapsed_milliseconds as f32
                / clip.duration_milliseconds as f32)
                .clamp(0.0, 1.0);
            transform.translation -=
                Vec3::from_array(motion.translation_from_first_to_last_frame) * fraction;
        }
    }
}
