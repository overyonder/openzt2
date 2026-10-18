//! Navigation consumes sampled animation motion once at the gameplay entity.

use bevy::animation::{animated_field, AnimationTargetId};
use bevy::math::Affine3A;
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationClipRecord;

#[derive(Component)]
pub struct ConsumedLocomotionRootMotion {
    playback_request_id: usize,
    actor_from_root_parent: Affine3A,
    translation: Vec3,
    rotation: Quat,
    mean_translation: Vec3,
}

impl ConsumedLocomotionRootMotion {
    #[must_use]
    pub const fn new(playback_request_id: usize, actor_from_root_parent: Affine3A) -> Self {
        Self {
            playback_request_id,
            actor_from_root_parent,
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            mean_translation: Vec3::ZERO,
        }
    }

    #[must_use]
    pub fn owns_playback(
        &self,
        controller: &super::animation_playback_controller_types::AnimationPlaybackController,
    ) -> bool {
        controller.explicit_clip_request_id == Some(self.playback_request_id)
    }

    pub const fn clear_sample(&mut self) {
        self.translation = Vec3::ZERO;
        self.rotation = Quat::IDENTITY;
        self.mean_translation = Vec3::ZERO;
    }

    pub const fn update_root_parent_transform(&mut self, actor_from_root_parent: Affine3A) {
        self.actor_from_root_parent = actor_from_root_parent;
    }

    #[must_use]
    pub const fn translation(&self) -> Vec3 {
        self.translation
    }
    #[must_use]
    pub const fn rotation(&self) -> Quat {
        self.rotation
    }

    #[must_use]
    pub const fn mean_translation(&self) -> Vec3 {
        self.mean_translation
    }

    pub fn sample_interval(
        &mut self,
        clip: &AnimationClip,
        root: &str,
        interval: std::ops::Range<f32>,
        complete_cycles: i64,
        duration_seconds: f32,
    ) {
        let target = AnimationTargetId::from_name(&Name::new(root.to_ascii_lowercase()));
        let sample = |time| {
            let translation = clip
                .sample_clamped(animated_field!(Transform::translation), target, time)
                .unwrap_or(Vec3::ZERO);
            let rotation = clip
                .sample_clamped(animated_field!(Transform::rotation), target, time)
                .unwrap_or(Quat::IDENTITY);
            Affine3A::from_rotation_translation(rotation, translation)
        };
        let first = sample(0.0);
        let last = sample(duration_seconds);
        self.mean_translation = self
            .actor_from_root_parent
            .transform_vector3(Vec3::from(last.translation - first.translation));
        // Lift wrapped samples into the continuous trajectory. Compose whole
        // cycles as rigid transforms so turning clips also retain their motion.
        let mut cycle = last * first.inverse();
        if complete_cycles < 0 {
            cycle = cycle.inverse();
        }
        let mut count = complete_cycles.unsigned_abs();
        let mut accumulated = Affine3A::IDENTITY;
        while count != 0 {
            if count & 1 != 0 {
                accumulated *= cycle;
            }
            cycle *= cycle;
            count >>= 1;
        }
        let previous = sample(interval.start);
        let next = accumulated * sample(interval.end);
        self.translation = self
            .actor_from_root_parent
            .transform_vector3(Vec3::from(next.translation - previous.translation));
        let parent_rotation = self
            .actor_from_root_parent
            .to_scale_rotation_translation()
            .1;
        let previous_rotation = previous.to_scale_rotation_translation().1;
        let next_rotation = next.to_scale_rotation_translation().1;
        self.rotation = (parent_rotation
            * next_rotation
            * previous_rotation.inverse()
            * parent_rotation.inverse())
        .normalize();
    }
}

/// Mean transformed clip velocity used by route planning and steering.
#[must_use]
pub fn clip_motion_velocity(
    clip: &AnimationClipRecord,
    root_parent: &GlobalTransform,
    playback_speed_permille: i16,
) -> Option<Vec3> {
    let metadata = clip.authored_animation_clip_metadata.as_ref()?;
    let motion = metadata.root_motion_transform?;
    metadata.root_motion_joint.as_ref()?;
    let duration =
        std::time::Duration::from_millis(u64::from(clip.duration_milliseconds)).as_secs_f32();
    if duration <= 0.0 {
        return None;
    }
    let rate = f32::from(playback_speed_permille) / 1000.0 * clip.authored_playback_rate();
    let velocity = root_parent.affine().transform_vector3(Vec3::from_array(
        motion.translation_from_first_to_last_frame,
    )) * (rate / duration);
    velocity.is_finite().then_some(velocity)
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationLocomotionMotionSampling;

#[cfg(test)]
mod tests;
