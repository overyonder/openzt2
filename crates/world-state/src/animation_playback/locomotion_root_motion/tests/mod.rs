use super::*;
use bevy::animation::animation_curves::{AnimatableCurve, AnimatableKeyframeCurve};

#[test]
fn translating_turns_continue_across_forward_and_reverse_loop_boundaries(
) -> Result<(), Box<dyn std::error::Error>> {
    let target = AnimationTargetId::from_name(&Name::new("root"));
    let mut clip = AnimationClip::default();
    clip.add_curve_to_target(
        target,
        AnimatableCurve::new(
            animated_field!(Transform::translation),
            AnimatableKeyframeCurve::new([(0.0, Vec3::ZERO), (1.0, Vec3::X * 2.0)])?,
        ),
    );
    clip.add_curve_to_target(
        target,
        AnimatableCurve::new(
            animated_field!(Transform::rotation),
            AnimatableKeyframeCurve::new([
                (0.0, Quat::IDENTITY),
                (1.0, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            ])?,
        ),
    );
    let mut motion = ConsumedLocomotionRootMotion::new(0, Affine3A::IDENTITY);
    motion.sample_interval(&clip, "ROOT", 0.75..0.25, 1, 1.0);
    // After crossing the corner, the next half metre points toward -Z.
    assert!(motion
        .translation()
        .abs_diff_eq(Vec3::new(0.5, 0.0, -0.5), 1e-5));
    assert!(motion
        .rotation()
        .abs_diff_eq(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4), 1e-5));
    motion.sample_interval(&clip, "root", 0.25..0.75, -1, 1.0);
    assert!(motion
        .translation()
        .abs_diff_eq(Vec3::new(-0.5, 0.0, -0.5), 1e-5));
    assert!(motion
        .rotation()
        .abs_diff_eq(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_4), 1e-5));
    // Two complete quarter-turn cycles end at (2, 0, -2), facing backward.
    motion.sample_interval(&clip, "root", 0.0..0.0, 2, 1.0);
    assert!(motion
        .translation()
        .abs_diff_eq(Vec3::new(2.0, 0.0, -2.0), 1e-5));
    assert!(motion
        .rotation()
        .abs_diff_eq(Quat::from_rotation_y(std::f32::consts::PI), 1e-5));
    Ok(())
}
