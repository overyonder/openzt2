use bevy::prelude::*;

use super::camera_runtime_state_types::{
    CameraBounds, CameraEasing, CameraTransition, CameraTuning, OverheadRig,
};

const ANGLE_TAU: f32 = std::f32::consts::TAU;

/// Moves the overhead camera focus along its screen-relative ground axes.
/// Positive input means camera-right/forward; source ground Y maps to Bevy -Z.
pub(super) fn apply_camera_relative_pan_to_overhead_focus(
    rig: &mut OverheadRig,
    camera_relative_displacement: Vec2,
) {
    rig.focus += Mat2::from_angle(rig.yaw)
        * Vec2::new(
            camera_relative_displacement.x,
            -camera_relative_displacement.y,
        );
}

pub(crate) fn clamp_rig(rig: &mut OverheadRig, tuning: &CameraTuning, bounds: &CameraBounds) {
    rig.focus = rig.focus.clamp(bounds.min, bounds.max);
    rig.distance = rig
        .distance
        .clamp(*tuning.distance.start(), *tuning.distance.end());
    if rig.distance > *tuning.distance.start() {
        rig.minimum_zoom_offset_m = 0.0;
    } else {
        rig.minimum_zoom_offset_m = rig.minimum_zoom_offset_m.min(0.0);
    }
    rig.pitch = rig.pitch.clamp(*tuning.pitch.start(), *tuning.pitch.end());
    let yaw_min = *tuning.yaw.start();
    let yaw_max = *tuning.yaw.end();
    rig.yaw = if yaw_max - yaw_min >= ANGLE_TAU - f32::EPSILON {
        yaw_min + (rig.yaw - yaw_min).rem_euclid(ANGLE_TAU)
    } else {
        rig.yaw.clamp(yaw_min, yaw_max)
    };
}

/// Applies radial zoom while retaining the original camera's signed residual
/// at minimum zoom. Outward zoom consumes that residual before increasing the
/// radial distance, avoiding a discontinuity at the boundary.
pub(crate) fn apply_overhead_zoom_delta(
    rig: &mut OverheadRig,
    tuning: &CameraTuning,
    camera_clearance_m: f32,
    delta_m: f32,
) {
    if !delta_m.is_finite() {
        return;
    }
    let minimum = *tuning.distance.start();
    let maximum = *tuning.distance.end();
    let requested = rig.distance + rig.minimum_zoom_offset_m + delta_m;
    if requested < minimum {
        rig.distance = minimum;
        let available_clearance =
            (minimum * rig.pitch.sin() - camera_clearance_m.max(0.0)).max(0.0);
        rig.minimum_zoom_offset_m = (requested - minimum).max(-available_clearance);
    } else {
        rig.distance = requested.clamp(minimum, maximum);
        rig.minimum_zoom_offset_m = 0.0;
    }
}

pub(crate) fn approach_scalar(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    let step = rate * dt;
    if (target - current).abs() <= step {
        target
    } else {
        current + (target - current).signum() * step
    }
}

/// Advances four independent directional ramps. The returned axis is sampled
/// before inactive-direction decay.
pub(crate) fn advance_pan_accumulators(
    accumulators: &mut [f32; 4],
    direction_magnitudes: [f32; 4],
    start_rate: f32,
    stop_rate: f32,
    delta_seconds: f32,
) -> Vec2 {
    let delta_seconds = delta_seconds.min(0.5);
    for (accumulator, magnitude) in accumulators.iter_mut().zip(direction_magnitudes) {
        if magnitude > 0.0 {
            *accumulator += start_rate * magnitude * delta_seconds;
        }
        *accumulator = accumulator.clamp(0.0, 1.0);
    }
    // The source ramps remain alive while an input is held and decay after it
    // is released, but an inactive direction contributes no translation. This
    // preserves the authored ease-in/re-press state without inventing an
    // observable ease-out: releasing the final movement input stops the
    // camera in the same update.
    let smoothed: [f32; 4] = std::array::from_fn(|index| {
        (direction_magnitudes[index] > 0.0)
            .then(|| (accumulators[index] * std::f32::consts::FRAC_PI_2).sin())
            .unwrap_or(0.0)
    });
    for (accumulator, magnitude) in accumulators.iter_mut().zip(direction_magnitudes) {
        if magnitude <= 0.0 {
            *accumulator = (*accumulator + stop_rate * delta_seconds).max(0.0);
        }
    }
    Vec2::new(smoothed[2] - smoothed[3], smoothed[0] - smoothed[1])
}

/// Which way the wheel turned, from the sign of its notches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelDirection {
    Up,
    Down,
}

impl WheelDirection {
    fn of(notches: f32) -> Option<Self> {
        if notches > 0.0 {
            Some(Self::Up)
        } else if notches < 0.0 {
            Some(Self::Down)
        } else {
            None
        }
    }

    fn sign(self) -> f32 {
        match self {
            Self::Up => 1.0,
            Self::Down => -1.0,
        }
    }
}

/// Zoom queued by wheel notches. An idle queue has no direction, so a notch in
/// either direction starts a new queue, and an opposite notch empties it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum WheelZoomQueue {
    #[default]
    Idle,
    Pending {
        direction: WheelDirection,
        seconds: f32,
    },
}

pub(crate) fn queue_wheel_zoom(queue: &mut WheelZoomQueue, wheel_notches: f32) {
    const SECONDS_PER_NOTCH: f32 = 0.08;
    let Some(notch) = WheelDirection::of(wheel_notches) else {
        return;
    };
    let impulse = wheel_notches.abs() * SECONDS_PER_NOTCH;
    *queue = match *queue {
        WheelZoomQueue::Pending { direction, seconds } if direction == notch => {
            WheelZoomQueue::Pending {
                direction,
                seconds: seconds + impulse,
            }
        }
        WheelZoomQueue::Pending { .. } => WheelZoomQueue::Idle,
        WheelZoomQueue::Idle => WheelZoomQueue::Pending {
            direction: notch,
            seconds: impulse,
        },
    };
}

pub(crate) fn consume_wheel_zoom(queue: &mut WheelZoomQueue, delta_seconds: f32) -> f32 {
    let WheelZoomQueue::Pending { direction, seconds } = *queue else {
        return 0.0;
    };
    let remaining = seconds - delta_seconds.min(seconds);
    *queue = if remaining > 0.0 {
        WheelZoomQueue::Pending {
            direction,
            seconds: remaining,
        }
    } else {
        WheelZoomQueue::Idle
    };
    direction.sign() * 2.0
}

pub(crate) fn overhead_eye_and_target(rig: &OverheadRig) -> (Vec3, Vec3) {
    let hierarchy_root = Vec3::new(rig.focus.x, rig.height_m, rig.focus.y);
    // The node assembly is root -> rotated vertical node ->
    // `-lookAtDist` node -> `-currZoom` node -> camera. Flatten that single
    // pitched radial branch into the authoritative Bevy camera transform.
    // `parentingOffset` belongs to a sibling node used by parented modes.
    let radial_distance = rig.look_at_distance_m + rig.distance;
    let horizontal = radial_distance * rig.pitch.cos();
    let eye = hierarchy_root
        + Vec3::Y * (rig.minimum_zoom_offset_m + rig.camera_ground_fit_offset_m)
        + Vec3::new(
            -rig.yaw.sin() * horizontal,
            radial_distance * rig.pitch.sin(),
            rig.yaw.cos() * horizontal,
        );
    let target = hierarchy_root
        + Vec3::Y
            * (rig.minimum_zoom_offset_m + rig.target_ground_fit_offset_m + rig.look_at_height_m);
    (eye, target)
}

pub(crate) fn overhead_pose(rig: &OverheadRig) -> Transform {
    let (eye, target) = overhead_eye_and_target(rig);
    Transform::from_translation(eye).looking_at(target, Vec3::Y)
}

pub(crate) fn advance_transition(
    transition: &mut CameraTransition,
    transform: &mut Transform,
    delta_seconds: f32,
) -> bool {
    transition.elapsed = (transition.elapsed + delta_seconds).min(transition.duration);
    let linear = (transition.elapsed / transition.duration).clamp(0.0, 1.0);
    let t = match transition.easing {
        CameraEasing::SmoothStep => linear * linear * (3.0 - 2.0 * linear),
    };
    transform.translation = transition
        .from
        .translation
        .lerp(transition.to.translation, t);
    transform.scale = transition.from.scale.lerp(transition.to.scale, t);
    transform.rotation = transition.from.rotation.slerp(transition.to.rotation, t);
    if transition.elapsed >= transition.duration {
        *transform = transition.to;
        true
    } else {
        false
    }
}
