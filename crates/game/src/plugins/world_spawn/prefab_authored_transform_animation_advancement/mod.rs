use bevy::{math::cubic_splines::CubicSegment, prelude::*};
use openzt2_game_data::scene_prefab::{
    PrefabControllerClock, PrefabRotationCurve, PrefabRotationKey, PrefabScalarCurve,
    PrefabTransformAnimation,
};

use super::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;

#[derive(Component, Debug, Clone)]
pub(super) struct PrefabAuthoredTransformAnimation {
    animation: PrefabTransformAnimation,
    app_init_clock_origin_s: Option<f32>, // Set on first advance for app-init controllers
}

impl PrefabAuthoredTransformAnimation {
    pub(super) fn from_authored_transform_animation(animation: &PrefabTransformAnimation) -> Self {
        Self {
            animation: animation.clone(),
            app_init_clock_origin_s: None,
        }
    }
}

pub(super) fn advance_authored_prefab_transform_animations_from_elapsed_time(
    time: Res<Time>,
    mut animations: Query<(
        &mut PrefabAuthoredTransformAnimation,
        &mut Transform,
        Option<&mut PrefabAuthoredBillboardOrientationMode>,
    )>,
) {
    let elapsed_s = time.elapsed_secs();
    for (mut animated, mut transform, billboard) in &mut animations {
        let clock = animated.animation.clock;
        let controller_elapsed_s = if clock.flags & 1 == 0 {
            elapsed_s
        } else {
            elapsed_s - *animated.app_init_clock_origin_s.get_or_insert(elapsed_s)
        };
        let animation = &animated.animation;
        let time_s = sample_controller_time(clock, controller_elapsed_s);
        if let Some([x, y, z]) = &animation.translation_m {
            transform.translation = Vec3::new(
                sample_scalar_curve(x, time_s, 0.0),
                sample_scalar_curve(y, time_s, 0.0),
                sample_scalar_curve(z, time_s, 0.0),
            );
        }
        if let Some(scale) = &animation.uniform_scale {
            transform.scale = Vec3::splat(sample_scalar_curve(scale, time_s, 1.0));
        }
        if let Some(rotation) = &animation.rotation {
            let rotation = sample_rotation_curve(rotation, time_s);
            // A billboard orients from its animated local rotation each frame.
            match billboard {
                Some(mut billboard) => {
                    if billboard.authored_local_rotation != rotation {
                        billboard.authored_local_rotation = rotation;
                    }
                }
                None => transform.rotation = rotation,
            }
        }
    }
}

fn sample_controller_time(clock: PrefabControllerClock, elapsed_s: f32) -> f32 {
    let start = clock.start_time_s;
    let stop = clock.stop_time_s;
    let duration = stop - start;
    if duration <= 0.0 {
        return start;
    }
    let time = elapsed_s.mul_add(clock.frequency, clock.phase);
    match (clock.flags >> 1) & 3 {
        0 => (start + (time - start).rem_euclid(duration)).clamp(start, stop),
        1 => {
            let phase = (time - start).rem_euclid(2.0 * duration);
            start
                + if phase <= duration {
                    phase
                } else {
                    2.0_f32.mul_add(duration, -phase)
                }
        }
        _ => time.clamp(start, stop),
    }
}

fn sample_scalar_curve(curve: &PrefabScalarCurve, time_s: f32, default: f32) -> f32 {
    let segments = &curve.segments;
    let Some(first) = segments.first() else {
        return default;
    };
    if time_s <= first.start_time_s {
        return first.coefficients[0];
    }
    let index = segments
        .partition_point(|segment| segment.start_time_s <= time_s)
        .saturating_sub(1);
    let segment = &segments[index];
    let fraction = segments.get(index + 1).map_or(0.0, |next| {
        (time_s - segment.start_time_s) / (next.start_time_s - segment.start_time_s)
    });
    CubicSegment {
        coeff: segment.coefficients,
    }
    .position(fraction)
}

fn sample_rotation_curve(rotation: &PrefabRotationCurve, time_s: f32) -> Quat {
    match rotation {
        PrefabRotationCurve::EulerAnglesXzy([x, z, y]) => {
            Quat::from_rotation_x(sample_scalar_curve(x, time_s, 0.0))
                * Quat::from_rotation_z(sample_scalar_curve(z, time_s, 0.0))
                * Quat::from_rotation_y(sample_scalar_curve(y, time_s, 0.0))
        }
        PrefabRotationCurve::Quaternions { keys, stepped } => {
            sample_rotation_keys(keys, *stepped, time_s)
        }
    }
}

fn sample_rotation_keys(keys: &[PrefabRotationKey], stepped: bool, time_s: f32) -> Quat {
    let Some(first) = keys.first() else {
        return Quat::IDENTITY;
    };
    if time_s <= first.time_s {
        return Quat::from_array(first.rotation_xyzw);
    }
    let index = keys
        .partition_point(|key| key.time_s <= time_s)
        .saturating_sub(1);
    let key = Quat::from_array(keys[index].rotation_xyzw);
    let Some(next) = keys.get(index + 1).filter(|_| !stepped) else {
        return key;
    };
    let span = next.time_s - keys[index].time_s;
    if span <= 0.0 {
        return Quat::from_array(next.rotation_xyzw);
    }
    key.slerp(
        Quat::from_array(next.rotation_xyzw),
        (time_s - keys[index].time_s) / span,
    )
}
