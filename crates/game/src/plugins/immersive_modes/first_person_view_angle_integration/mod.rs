use std::ops::RangeInclusive;

use bevy::prelude::*;

use super::immersive_mode_control_types::FirstPersonViewAngles;

pub(super) fn integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds(
    view_angles: &mut FirstPersonViewAngles,
    analog_look_input: Vec2,
    digital_horizontal_turn_input: f32,
    turn_speed_radians_per_second: f32,
    allowed_pitch_radians: &RangeInclusive<f32>,
    allowed_yaw_radians: &RangeInclusive<f32>,
    elapsed_seconds: f32,
) {
    let elapsed_seconds = elapsed_seconds.max(0.0);
    // A camera looks along -Z: positive Y rotation turns left. Input X and
    // RotateRight are positive toward screen-right, hence the subtraction.
    view_angles.yaw -= (analog_look_input.x + digital_horizontal_turn_input).clamp(-1.0, 1.0)
        * turn_speed_radians_per_second
        * elapsed_seconds;
    view_angles.pitch = (view_angles.pitch
        - analog_look_input.y * turn_speed_radians_per_second * elapsed_seconds)
        .clamp(*allowed_pitch_radians.start(), *allowed_pitch_radians.end());
    let minimum_yaw_radians = *allowed_yaw_radians.start();
    let maximum_yaw_radians = *allowed_yaw_radians.end();
    view_angles.yaw =
        if maximum_yaw_radians - minimum_yaw_radians >= std::f32::consts::TAU - f32::EPSILON {
            minimum_yaw_radians
                + (view_angles.yaw - minimum_yaw_radians).rem_euclid(std::f32::consts::TAU)
        } else {
            view_angles
                .yaw
                .clamp(minimum_yaw_radians, maximum_yaw_radians)
        };
}
