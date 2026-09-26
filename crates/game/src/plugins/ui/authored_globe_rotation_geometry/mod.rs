use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;
use std::f32::consts::TAU;

pub(super) fn source_rotation_to_bevy(latitude: f32, longitude: f32) -> Quat {
    // The constructors store X(latitude) and Z(-longitude) using
    // Gamebryo's matrix convention. Applied to the map-dot model's source -Y
    // radial axis, those matrices are the equivalent of Bevy column-vector
    // rotations X(-latitude), then Z(longitude). This is independently fixed
    // by the Earth mesh UV seam: source -Y is (0°, 0°), source +X is
    // (90° E, 0°), and source +Z is the north pole.
    let source = Mat3::from_rotation_z(longitude) * Mat3::from_rotation_x(-latitude);
    let bevy_basis = |source_axis: Vec3| {
        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
            source_axis.to_array(),
        ))
    };
    let source_to_bevy = Mat3::from_cols(
        bevy_basis(Vec3::X),
        bevy_basis(Vec3::Y),
        bevy_basis(Vec3::Z),
    );
    // The proper rotation basis is orthogonal, so its inverse is its
    // transpose. Applying the forward conversion twice adds a half turn.
    Quat::from_mat3(&(source_to_bevy * source * source_to_bevy.transpose()))
}

pub(super) fn constrained_globe_angles(rotation: Quat) -> (f32, f32) {
    // Rx(pitch) * Ry(yaw) keeps the local Y axis independent of yaw.
    // Recover the two constrained angles directly: general XYZ Euler
    // decomposition folds yaw at +/-pi/2 and introduces a discarded roll.
    let right = rotation * Vec3::X;
    let up = rotation * Vec3::Y;
    let back = rotation * Vec3::Z;
    (
        back.x.atan2(right.x),
        up.z.atan2(up.y).clamp(-FRAC_PI_2, FRAC_PI_2),
    )
}

pub(super) fn globe_pitch_after_pointer_drag(pitch: f32, downward_delta: f32) -> f32 {
    (pitch + downward_delta).clamp(-FRAC_PI_2, FRAC_PI_2)
}

pub(super) fn globe_rotation(yaw: f32, pitch: f32) -> Quat {
    // Apply yaw first, then pitch about the fixed screen-horizontal/world X
    // axis. Composing these as Y then X makes vertical input rotate around a
    // yawed local axis, which presents as roll.
    Quat::from_euler(EulerRot::XYZ, pitch, yaw, 0.0)
}

pub(super) fn globe_alignment_angles(
    source: Vec3,
    target: Vec3,
    current_yaw: f32,
    current_pitch: f32,
) -> (f32, f32) {
    let source = source.normalize_or_zero();
    let target = target.normalize_or_zero();
    let horizontal_radius = source.x.hypot(source.z);
    if horizontal_radius <= f32::EPSILON {
        let yaw = current_yaw;
        let yawed = Quat::from_rotation_y(yaw) * source;
        let pitch = normalize_angle(target.z.atan2(target.y) - yawed.z.atan2(yawed.y))
            .clamp(-FRAC_PI_2, FRAC_PI_2);
        return (yaw, pitch);
    }
    let phase = source.z.atan2(source.x);
    let offset = (target.x / horizontal_radius).clamp(-1.0, 1.0).acos();
    [phase + offset, phase - offset]
        .into_iter()
        .map(normalize_angle)
        .filter_map(|yaw| {
            let yawed = Quat::from_rotation_y(yaw) * source;
            let pitch = normalize_angle(target.z.atan2(target.y) - yawed.z.atan2(yawed.y));
            (pitch.abs() <= FRAC_PI_2).then_some((yaw, pitch))
        })
        .min_by(|(left_yaw, left_pitch), (right_yaw, right_pitch)| {
            (shortest_angle_delta(current_yaw, *left_yaw).abs()
                + (left_pitch - current_pitch).abs())
            .total_cmp(
                &(shortest_angle_delta(current_yaw, *right_yaw).abs()
                    + (right_pitch - current_pitch).abs()),
            )
        })
        .unwrap_or((current_yaw, current_pitch))
}

pub(super) fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    normalize_angle(to - from)
}

pub(super) fn normalize_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod globe_rotation_regression_tests {
    use super::{constrained_globe_angles, globe_rotation, normalize_angle};
    use bevy::prelude::Vec3;

    #[test]
    fn vertical_globe_drag_moves_front_surface_with_pointer() {
        for delta in [-0.2_f32, 0.2] {
            let pitch = super::globe_pitch_after_pointer_drag(0.0, delta);
            let front = globe_rotation(0.0, pitch) * Vec3::Z;
            assert!(front.y * delta < 0.0);
        }
        assert_eq!(
            super::globe_pitch_after_pointer_drag(1.5, 1.0),
            std::f32::consts::FRAC_PI_2
        );
    }

    #[test]
    fn constrained_globe_rotation_survives_repeated_full_turns_in_both_directions() {
        for pitch in [-1.5, -0.6, 0.0, 0.6, 1.5] {
            for step in [-0.02, 0.02] {
                let mut rotation = globe_rotation(0.0, pitch);
                let mut expected_yaw = 0.0;
                for _ in 0..2_000 {
                    let (yaw, recovered_pitch) = constrained_globe_angles(rotation);
                    expected_yaw = normalize_angle(expected_yaw + step);
                    rotation = globe_rotation(yaw + step, recovered_pitch);
                    let expected = globe_rotation(expected_yaw, pitch);
                    assert!((rotation * Vec3::Z).distance(expected * Vec3::Z) < 0.001);
                    assert!((recovered_pitch - pitch).abs() < 0.001);
                }
            }
        }
    }
}
