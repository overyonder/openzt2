//! NetImmerse Bezier and tension-bias-continuity curve sampling.

use super::conversion_error::ConversionError;

const NETIMMERSE_CURVE_SAMPLE_INTERVAL_SECONDS: f32 = 0.008;

pub(super) struct NetImmerseSourceKeyframe<const N: usize> {
    pub(super) time_seconds: f32,
    pub(super) value: [f32; N],
    pub(super) outgoing_bezier_control_value: Option<[f32; N]>,
    pub(super) incoming_bezier_control_value: Option<[f32; N]>,
    pub(super) tension_bias_continuity: Option<[f32; 3]>,
}

pub(super) fn sample_netimmerse_keyframes<const N: usize>(
    source_interpolation_kind: Option<u32>,
    source_keyframes: &[NetImmerseSourceKeyframe<N>],
) -> Result<Vec<(f32, [f32; N])>, ConversionError> {
    match source_interpolation_kind {
        None | Some(1 | 5) => Ok(source_keyframes
            .iter()
            .map(|source_keyframe| (source_keyframe.time_seconds, source_keyframe.value))
            .collect()),
        Some(2) => sample_netimmerse_curve_segments(
            source_keyframes,
            |segment_start_keyframe, segment_end_keyframe, segment_fraction| {
                let inverse_segment_fraction = 1.0 - segment_fraction;
                let outgoing_control_value = segment_start_keyframe
                    .outgoing_bezier_control_value
                    .unwrap_or(segment_start_keyframe.value);
                let incoming_control_value = segment_end_keyframe
                    .incoming_bezier_control_value
                    .unwrap_or(segment_end_keyframe.value);
                std::array::from_fn(|component_index| {
                    inverse_segment_fraction.powi(3) * segment_start_keyframe.value[component_index]
                        + 3.0
                            * inverse_segment_fraction.powi(2)
                            * segment_fraction
                            * outgoing_control_value[component_index]
                        + 3.0
                            * inverse_segment_fraction
                            * segment_fraction.powi(2)
                            * incoming_control_value[component_index]
                        + segment_fraction.powi(3) * segment_end_keyframe.value[component_index]
                })
            },
        ),
        Some(3) => sample_netimmerse_curve_segments_by_index(
            source_keyframes,
            |segment_start_keyframe_index, segment_fraction| {
                interpolate_netimmerse_tension_bias_continuity_value(
                    source_keyframes,
                    segment_start_keyframe_index,
                    segment_fraction,
                )
            },
        ),
        _ => Err(ConversionError::InvalidValue(
            "unsupported KF interpolation",
        )),
    }
}

fn sample_netimmerse_curve_segments<const N: usize>(
    source_keyframes: &[NetImmerseSourceKeyframe<N>],
    interpolate_segment_value: impl Fn(
        &NetImmerseSourceKeyframe<N>,
        &NetImmerseSourceKeyframe<N>,
        f32,
    ) -> [f32; N],
) -> Result<Vec<(f32, [f32; N])>, ConversionError> {
    sample_netimmerse_curve_segments_by_index(
        source_keyframes,
        |segment_start_keyframe_index, segment_fraction| {
            interpolate_segment_value(
                &source_keyframes[segment_start_keyframe_index],
                &source_keyframes[segment_start_keyframe_index + 1],
                segment_fraction,
            )
        },
    )
}

fn sample_netimmerse_curve_segments_by_index<const N: usize>(
    source_keyframes: &[NetImmerseSourceKeyframe<N>],
    interpolate_segment_value: impl Fn(usize, f32) -> [f32; N],
) -> Result<Vec<(f32, [f32; N])>, ConversionError> {
    let Some(first_source_keyframe) = source_keyframes.first() else {
        return Ok(Vec::new());
    };
    let mut sampled_keyframes = vec![(
        first_source_keyframe.time_seconds,
        first_source_keyframe.value,
    )];
    for segment_start_keyframe_index in 0..source_keyframes.len().saturating_sub(1) {
        let segment_start_time_seconds =
            source_keyframes[segment_start_keyframe_index].time_seconds;
        let segment_end_time_seconds =
            source_keyframes[segment_start_keyframe_index + 1].time_seconds;
        if segment_end_time_seconds < segment_start_time_seconds {
            return Err(ConversionError::InvalidValue(
                "KF key times are not monotonic",
            ));
        }
        let segment_sample_count = ((segment_end_time_seconds - segment_start_time_seconds)
            / NETIMMERSE_CURVE_SAMPLE_INTERVAL_SECONDS)
            .ceil()
            .max(1.0) as u32;
        (1..=segment_sample_count).for_each(|segment_sample_index| {
            let segment_fraction = segment_sample_index as f32 / segment_sample_count as f32;
            sampled_keyframes.push((
                segment_start_time_seconds
                    + (segment_end_time_seconds - segment_start_time_seconds) * segment_fraction,
                interpolate_segment_value(segment_start_keyframe_index, segment_fraction),
            ));
        });
    }
    Ok(sampled_keyframes)
}

fn interpolate_netimmerse_tension_bias_continuity_value<const N: usize>(
    source_keyframes: &[NetImmerseSourceKeyframe<N>],
    segment_start_keyframe_index: usize,
    segment_fraction: f32,
) -> [f32; N] {
    let preceding_keyframe = source_keyframes
        .get(segment_start_keyframe_index.wrapping_sub(1))
        .unwrap_or(&source_keyframes[segment_start_keyframe_index]);
    let segment_start_keyframe = &source_keyframes[segment_start_keyframe_index];
    let segment_end_keyframe = &source_keyframes[segment_start_keyframe_index + 1];
    let following_keyframe = source_keyframes
        .get(segment_start_keyframe_index + 2)
        .unwrap_or(segment_end_keyframe);
    let [start_tension, start_bias, start_continuity] = segment_start_keyframe
        .tension_bias_continuity
        .unwrap_or([0.0; 3]);
    let [end_tension, end_bias, end_continuity] = segment_end_keyframe
        .tension_bias_continuity
        .unwrap_or([0.0; 3]);
    let squared_segment_fraction = segment_fraction * segment_fraction;
    let cubed_segment_fraction = squared_segment_fraction * segment_fraction;
    std::array::from_fn(|component_index| {
        let preceding_value_difference = segment_start_keyframe.value[component_index]
            - preceding_keyframe.value[component_index];
        let segment_value_difference = segment_end_keyframe.value[component_index]
            - segment_start_keyframe.value[component_index];
        let following_value_difference =
            following_keyframe.value[component_index] - segment_end_keyframe.value[component_index];
        let outgoing_tangent = (1.0 - start_tension)
            * ((1.0 + start_continuity) * (1.0 + start_bias) * preceding_value_difference
                + (1.0 - start_continuity) * (1.0 - start_bias) * segment_value_difference)
            * 0.5;
        let incoming_tangent = (1.0 - end_tension)
            * ((1.0 - end_continuity) * (1.0 + end_bias) * segment_value_difference
                + (1.0 + end_continuity) * (1.0 - end_bias) * following_value_difference)
            * 0.5;
        (2.0 * cubed_segment_fraction - 3.0 * squared_segment_fraction + 1.0)
            * segment_start_keyframe.value[component_index]
            + (cubed_segment_fraction - 2.0 * squared_segment_fraction + segment_fraction)
                * outgoing_tangent
            + (-2.0 * cubed_segment_fraction + 3.0 * squared_segment_fraction)
                * segment_end_keyframe.value[component_index]
            + (cubed_segment_fraction - squared_segment_fraction) * incoming_tangent
    })
}

pub(super) fn sample_netimmerse_linear_scalar_keyframes(
    sampled_keyframes: &[(f32, f32)],
    requested_time_seconds: f32,
) -> f32 {
    let Some(first_sampled_keyframe) = sampled_keyframes.first() else {
        return 0.0;
    };
    let Some(containing_keyframe_pair) = sampled_keyframes
        .windows(2)
        .find(|keyframe_pair| requested_time_seconds <= keyframe_pair[1].0)
    else {
        return sampled_keyframes
            .last()
            .map_or(first_sampled_keyframe.1, |sampled_keyframe| {
                sampled_keyframe.1
            });
    };
    let keyframe_time_span_seconds = containing_keyframe_pair[1].0 - containing_keyframe_pair[0].0;
    if keyframe_time_span_seconds <= 0.0 {
        containing_keyframe_pair[1].1
    } else {
        containing_keyframe_pair[0].1
            + (containing_keyframe_pair[1].1 - containing_keyframe_pair[0].1)
                * ((requested_time_seconds - containing_keyframe_pair[0].0)
                    / keyframe_time_span_seconds)
    }
}

pub(super) fn convert_netimmerse_xyz_euler_angles_to_quaternion(
    x_axis_radians: f32,
    y_axis_radians: f32,
    z_axis_radians: f32,
) -> [f32; 4] {
    let (x_axis_half_angle_sine, x_axis_half_angle_cosine) = (x_axis_radians * 0.5).sin_cos();
    let (y_axis_half_angle_sine, y_axis_half_angle_cosine) = (y_axis_radians * 0.5).sin_cos();
    let (z_axis_half_angle_sine, z_axis_half_angle_cosine) = (z_axis_radians * 0.5).sin_cos();
    normalize_netimmerse_rotation_quaternion([
        x_axis_half_angle_sine * y_axis_half_angle_cosine * z_axis_half_angle_cosine
            + x_axis_half_angle_cosine * y_axis_half_angle_sine * z_axis_half_angle_sine,
        x_axis_half_angle_cosine * y_axis_half_angle_sine * z_axis_half_angle_cosine
            - x_axis_half_angle_sine * y_axis_half_angle_cosine * z_axis_half_angle_sine,
        x_axis_half_angle_cosine * y_axis_half_angle_cosine * z_axis_half_angle_sine
            + x_axis_half_angle_sine * y_axis_half_angle_sine * z_axis_half_angle_cosine,
        x_axis_half_angle_cosine * y_axis_half_angle_cosine * z_axis_half_angle_cosine
            - x_axis_half_angle_sine * y_axis_half_angle_sine * z_axis_half_angle_sine,
    ])
}

pub(super) fn normalize_netimmerse_rotation_quaternion(rotation_quaternion: [f32; 4]) -> [f32; 4] {
    let rotation_quaternion_length = rotation_quaternion
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if rotation_quaternion_length > f32::EPSILON {
        rotation_quaternion.map(|component| component / rotation_quaternion_length)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub(super) const fn convert_wxyz_quaternion_to_xyzw(wxyz_quaternion: [f32; 4]) -> [f32; 4] {
    [
        wxyz_quaternion[1],
        wxyz_quaternion[2],
        wxyz_quaternion[3],
        wxyz_quaternion[0],
    ]
}
