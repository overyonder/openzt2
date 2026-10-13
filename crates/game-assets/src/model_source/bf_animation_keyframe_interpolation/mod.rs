//! Blue Fang scalar-axis interpolation and quaternion construction.

use std::collections::BTreeSet;

use super::{
    bf_animation_source_types::{BlueFangAnimationProperty, BlueFangAnimationTrack},
    conversion_error::ConversionError,
};

pub(super) fn combine_blue_fang_scalar_axes_into_vector_track(
    axis_keyframes: &[Option<Vec<(u16, f32)>>; 3],
    animated_property: BlueFangAnimationProperty,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let keyframe_times_milliseconds = axis_keyframes
        .iter()
        .filter_map(Option::as_ref)
        .flatten()
        .map(|keyframe| keyframe.0)
        .collect::<BTreeSet<_>>();
    if keyframe_times_milliseconds.is_empty() {
        return Err(ConversionError::InvalidSource(
            "BF axis track has no keyframes",
        ));
    }
    Ok(BlueFangAnimationTrack {
        animated_property,
        keyframe_times_seconds: keyframe_times_milliseconds
            .iter()
            .map(|time_milliseconds| f32::from(*time_milliseconds) / 1000.0)
            .collect(),
        keyframe_values: keyframe_times_milliseconds
            .into_iter()
            .flat_map(|time_milliseconds| {
                axis_keyframes.each_ref().map(|axis_keyframes| {
                    sample_blue_fang_scalar_axis_at_time(
                        axis_keyframes.as_deref(),
                        time_milliseconds,
                    )
                })
            })
            .collect(),
    })
}

pub(super) fn combine_blue_fang_euler_axes_into_quaternion_track(
    axis_keyframes: &[Option<Vec<(u16, f32)>>; 3],
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let combined_euler_axis_track = combine_blue_fang_scalar_axes_into_vector_track(
        axis_keyframes,
        BlueFangAnimationProperty::Translation,
    )?;
    Ok(BlueFangAnimationTrack {
        animated_property: BlueFangAnimationProperty::Rotation,
        keyframe_times_seconds: combined_euler_axis_track.keyframe_times_seconds,
        keyframe_values: combined_euler_axis_track
            .keyframe_values
            .chunks_exact(3)
            .flat_map(|euler_angles| {
                convert_blue_fang_xyz_euler_angles_to_quaternion(
                    euler_angles[0],
                    euler_angles[1],
                    euler_angles[2],
                )
            })
            .collect(),
    })
}

fn sample_blue_fang_scalar_axis_at_time(
    keyframes: Option<&[(u16, f32)]>,
    time_milliseconds: u16,
) -> f32 {
    let Some(keyframes) = keyframes else {
        return 0.0;
    };
    let right_keyframe_index =
        keyframes.partition_point(|keyframe| keyframe.0 <= time_milliseconds);
    match (
        right_keyframe_index
            .checked_sub(1)
            .and_then(|left_keyframe_index| keyframes.get(left_keyframe_index)),
        keyframes.get(right_keyframe_index),
    ) {
        (Some(left_keyframe), Some(right_keyframe)) if right_keyframe.0 != left_keyframe.0 => {
            let interpolation_fraction = f32::from(time_milliseconds - left_keyframe.0)
                / f32::from(right_keyframe.0 - left_keyframe.0);
            left_keyframe.1 + (right_keyframe.1 - left_keyframe.1) * interpolation_fraction
        }
        (Some(left_keyframe), _) => left_keyframe.1,
        (_, Some(right_keyframe)) => right_keyframe.1,
        _ => 0.0,
    }
}

pub(super) fn normalize_blue_fang_rotation_quaternion(
    mut rotation_quaternion: [f32; 4],
) -> Result<[f32; 4], ConversionError> {
    let quaternion_length = rotation_quaternion
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if !quaternion_length.is_finite() {
        return Err(ConversionError::InvalidValue("non-finite BF quaternion"));
    }
    if quaternion_length < 0.0001 {
        return Ok([0.0, 0.0, 0.0, 1.0]);
    }
    rotation_quaternion
        .iter_mut()
        .for_each(|component| *component /= quaternion_length);
    Ok(rotation_quaternion)
}

fn convert_blue_fang_xyz_euler_angles_to_quaternion(
    x_axis_radians: f32,
    y_axis_radians: f32,
    z_axis_radians: f32,
) -> [f32; 4] {
    let (x_half_angle_sine, x_half_angle_cosine) = (x_axis_radians * 0.5).sin_cos();
    let (y_half_angle_sine, y_half_angle_cosine) = (y_axis_radians * 0.5).sin_cos();
    let (z_half_angle_sine, z_half_angle_cosine) = (z_axis_radians * 0.5).sin_cos();
    [
        x_half_angle_sine * y_half_angle_cosine * z_half_angle_cosine
            + x_half_angle_cosine * y_half_angle_sine * z_half_angle_sine,
        x_half_angle_cosine * y_half_angle_sine * z_half_angle_cosine
            - x_half_angle_sine * y_half_angle_cosine * z_half_angle_sine,
        x_half_angle_cosine * y_half_angle_cosine * z_half_angle_sine
            + x_half_angle_sine * y_half_angle_sine * z_half_angle_cosine,
        x_half_angle_cosine * y_half_angle_cosine * z_half_angle_cosine
            - x_half_angle_sine * y_half_angle_sine * z_half_angle_sine,
    ]
}
