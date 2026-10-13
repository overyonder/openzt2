//! Transfers only explicitly consumed locomotion root translation to gameplay.

use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationClipRecord;

#[derive(Component)]
pub struct ConsumedLocomotionRootMotion {
    pub playback_request_id: usize,
}

/// Transformed clip displacement, multiplied by
/// the controller playback factor, divided by clip duration. Translation is
/// already in Bevy coordinates at the typed animation-loader boundary.
pub fn clip_motion_velocity(
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
