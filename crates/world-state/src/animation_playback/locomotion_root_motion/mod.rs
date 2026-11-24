//! Navigation consumes sampled animation motion once at the gameplay entity.

use bevy::animation::{animated_field, AnimationTargetId};
use bevy::math::Affine3A;
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_clip_source_queries::LoadedAnimationClip;

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
        cycle: &LocomotionRootMotionCycle,
        interval: std::ops::Range<f32>,
        complete_cycles: i64,
    ) {
        let target = cycle.target;
        self.mean_translation = self
            .actor_from_root_parent
            .transform_vector3(cycle.translation);
        // Lift wrapped samples into the continuous trajectory. Compose whole
        // cycles as rigid transforms so turning clips also retain their motion.
        let mut cycle = cycle.motion;
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
        let previous = sample_root_at(clip, target, interval.start);
        let next = accumulated * sample_root_at(clip, target, interval.end);
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

/// Source-space motion across the bound clip's effective loop window.
#[derive(Clone, Copy, Debug)]
pub struct LocomotionRootMotionCycle {
    target: AnimationTargetId,
    motion: Affine3A,
    translation: Vec3,
}

impl LocomotionRootMotionCycle {
    pub fn from_clip(clip: &AnimationClip, root: &str, window: std::ops::Range<f32>) -> Self {
        let target = AnimationTargetId::from_name(&Name::new(root.to_ascii_lowercase()));
        let first = sample_root_at(clip, target, window.start);
        let last = sample_root_at(clip, target, window.end);
        Self {
            target,
            motion: last * first.inverse(),
            translation: (last.translation - first.translation).into(),
        }
    }
}

fn sample_root_at(clip: &AnimationClip, target: AnimationTargetId, time: f32) -> Affine3A {
    let translation = clip
        .sample_clamped(animated_field!(Transform::translation), target, time)
        .unwrap_or(Vec3::ZERO);
    let rotation = clip
        .sample_clamped(animated_field!(Transform::rotation), target, time)
        .unwrap_or(Quat::IDENTITY);
    Affine3A::from_rotation_translation(rotation, translation)
}

/// Mean transformed clip velocity used by route planning and steering.
#[must_use]
pub fn clip_motion_velocity(
    clip: &LoadedAnimationClip,
    root_parent: &GlobalTransform,
    playback_speed_permille: i16,
) -> Option<Vec3> {
    let metadata = clip.metadata?;
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
