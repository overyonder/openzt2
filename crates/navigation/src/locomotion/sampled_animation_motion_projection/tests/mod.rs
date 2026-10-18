use super::*;
use bevy::animation::animation_curves::{AnimatableCurve, AnimatableKeyframeCurve};
use bevy::animation::{animated_field, AnimationTargetId};
use bevy::math::Affine3A;
use openzt2_world_state::animation_playback::animation_playback_controller_types::{
    AnimationPlaybackClock, AnimationPlaybackRepetitionPolicy, AnimationPlaybackState,
};

#[test]
fn a_rotated_scaled_model_follows_the_requested_world_direction(
) -> Result<(), Box<dyn std::error::Error>> {
    let target = AnimationTargetId::from_name(&Name::new("root"));
    let mut clip = AnimationClip::default();
    clip.add_curve_to_target(
        target,
        AnimatableCurve::new(
            animated_field!(Transform::translation),
            AnimatableKeyframeCurve::new([(0.0, Vec3::ZERO), (1.0, Vec3::Z * 2.0)])?,
        ),
    );
    let parent = Affine3A::from_scale_rotation_translation(
        Vec3::splat(2.0),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Vec3::ZERO,
    );
    let mut motion = ConsumedLocomotionRootMotion::new(1, parent);
    motion.sample_interval(&clip, "root", 0.0..0.25, 0, 1.0);
    let controller = AnimationPlaybackController {
        playback_generation: 0,
        explicit_clip_request_id: Some(1),
        animation_set_asset: Handle::default(),
        animation_clip_asset_key: "walk".to_owned(),
        elapsed_milliseconds: 250,
        playback_speed_permille: 1000,
        playback_state: AnimationPlaybackState::Playing,
        playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
        playback_clock: AnimationPlaybackClock::Simulation,
    };
    // The model travels two metres per second, scaled to four and rotated
    // toward +X. Steering requests +Z, so its one-metre step must turn to +Z.
    let (velocity, orientation) = project_sampled_animation_motion_to_steering(
        Vec3::Z * 4.0,
        4.0,
        0.25,
        &Transform::IDENTITY,
        Some((&motion, &controller)),
    );
    assert!(velocity.abs_diff_eq(Vec3::Z * 4.0, 1e-5));
    assert!((orientation * Vec3::X).abs_diff_eq(Vec3::Z, 1e-5));
    Ok(())
}
