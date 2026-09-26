//! Transfers only explicitly consumed locomotion root translation to gameplay.

use super::{
    animation_joint_target_types::AnimationJointTarget,
    animation_playback_controller_types::AnimationPlaybackController,
};
use crate::assets::animation::animation_set_asset_types::{AnimationClipRecord, AnimationSetAsset};
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct ConsumedLocomotionRootMotion {
    pub(crate) playback_request_id: usize,
}

/// Transformed clip displacement, multiplied by
/// the controller playback factor, divided by clip duration. Translation is
/// already in Bevy coordinates at the typed animation-loader boundary.
pub(crate) fn clip_motion_velocity(
    clip: &AnimationClipRecord,
    root_parent: &GlobalTransform,
    playback_speed_permille: i16,
) -> Option<Vec3> {
    let metadata = clip.authored_animation_clip_metadata.as_ref()?;
    let motion = metadata.root_motion_transform?;
    metadata.root_motion_joint.as_ref()?;
    let duration = clip.duration_milliseconds as f32 / 1000.0;
    if duration <= 0.0 {
        return None;
    }
    let rate = f32::from(playback_speed_permille) / 1000.0 * clip.authored_playback_rate();
    let velocity = root_parent.affine().transform_vector3(Vec3::from_array(
        motion.translation_from_first_to_last_frame,
    )) * (rate / duration);
    velocity.is_finite().then_some(velocity)
}

pub(super) fn remove_consumed_root_trajectory(
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
