use bevy::prelude::*;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

/// A presentation controller supplies samples; the actor retains movement authority.
#[derive(Component, Clone, Copy)]
pub(super) struct LocomotionAnimationSource {
    pub controller: Entity,
    pub mean_speed_mps: f32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct LocomotionAnimationSamples<'w, 's> {
    controllers: Query<
        'w,
        's,
        (
            &'static ConsumedLocomotionRootMotion,
            &'static AnimationPlaybackController,
        ),
    >,
}

impl LocomotionAnimationSamples<'_, '_> {
    pub(super) fn get(
        &self,
        actor: Entity,
        source: Option<&LocomotionAnimationSource>,
    ) -> Option<(&ConsumedLocomotionRootMotion, &AnimationPlaybackController)> {
        self.controllers
            .get(source.map_or(actor, |source| source.controller))
            .ok()
    }
}

pub(super) fn project_sampled_animation_motion_to_steering(
    desired_velocity: Vec3,
    mean_speed: f32,
    delta_seconds: f32,
    transform: &Transform,
    playback: Option<(&ConsumedLocomotionRootMotion, &AnimationPlaybackController)>,
) -> (Vec3, Quat) {
    let Some((motion, controller)) =
        playback.filter(|(motion, controller)| motion.owns_playback(controller))
    else {
        return (desired_velocity, Quat::IDENTITY);
    };
    if delta_seconds <= 0.0 || mean_speed <= 0.0 {
        return (Vec3::ZERO, Quat::IDENTITY);
    }
    let direction = desired_velocity.normalize_or_zero();
    if direction == Vec3::ZERO {
        return (Vec3::ZERO, Quat::IDENTITY);
    }
    let reference = transform.scale
        * motion.mean_translation()
        * f32::from(controller.playback_speed_permille.signum());
    let reference_correction = Quat::from_rotation_y(-reference.x.atan2(reference.z));
    let heading = Quat::from_rotation_y(direction.x.atan2(direction.z)) * reference_correction;
    let fraction = (desired_velocity.length() / mean_speed).min(1.0);
    let velocity = heading * (transform.scale * motion.translation()) * (fraction / delta_seconds);
    (velocity, reference_correction * motion.rotation())
}

#[cfg(test)]
mod tests;
