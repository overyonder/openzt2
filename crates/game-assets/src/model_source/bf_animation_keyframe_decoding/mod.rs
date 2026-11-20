//! Blue Fang translation, rotation, scale, and scalar-axis keyframe decoding.

use super::{
    bf_animation_cubic_keyframe_sampling::BlueFangScalarCurve,
    bf_animation_keyframe_interpolation::normalize_blue_fang_rotation_quaternion,
    bf_animation_primitive_reading::{read_blue_fang_i16, read_blue_fang_u16, read_blue_fang_u8},
    bf_animation_quaternion_keyframe_sampling::BlueFangQuaternionCurve,
    bf_animation_source_types::BlueFangAnimationTrack,
    conversion_error::ConversionError,
};

pub(super) fn parse_blue_fang_translation_vector_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let keyframes = (0..keyframe_count)
        .map(|_| {
            let keyframe_time_milliseconds = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
            let translation_components = [
                read_blue_fang_i16(source_bytes, source_byte_cursor)?,
                read_blue_fang_i16(source_bytes, source_byte_cursor)?,
                read_blue_fang_i16(source_bytes, source_byte_cursor)?,
            ]
            .map(|encoded_component| f32::from(encoded_component) / 1000.0);
            let mut incoming = [0.0; 3];
            let mut outgoing = [0.0; 3];
            if includes_control_values {
                for value in incoming.iter_mut().chain(outgoing.iter_mut()) {
                    *value =
                        f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0;
                }
            }
            Ok((
                keyframe_time_milliseconds,
                translation_components,
                incoming,
                outgoing,
            ))
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    Ok(BlueFangAnimationTrack::Translation(
        BlueFangScalarCurve::new(keyframes, includes_control_values),
    ))
}

pub(super) fn parse_blue_fang_scalar_axis_keyframes(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangScalarCurve<1>, ConversionError> {
    let keyframes = (0..keyframe_count)
        .map(|_| {
            let scalar_axis_keyframe = (
                read_blue_fang_u16(source_bytes, source_byte_cursor)?,
                f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0,
            );
            let (incoming, outgoing) = if includes_control_values {
                (
                    f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0,
                    f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0,
                )
            } else {
                (0.0, 0.0)
            };
            Ok((
                scalar_axis_keyframe.0,
                [scalar_axis_keyframe.1],
                [incoming],
                [outgoing],
            ))
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    Ok(BlueFangScalarCurve::new(keyframes, includes_control_values))
}

pub(super) fn parse_blue_fang_quaternion_rotation_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let keyframes = (0..keyframe_count)
        .map(|_| {
            let keyframe_time_milliseconds = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
            let mut rotation_quaternion = [0.0; 4];
            rotation_quaternion
                .iter_mut()
                .try_for_each(|quaternion_component| {
                    *quaternion_component =
                        f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 10_000.0;
                    Ok::<_, ConversionError>(())
                })?;
            let mut tension_continuity_bias = [0.0; 3];
            if includes_control_values {
                for value in &mut tension_continuity_bias {
                    *value =
                        f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0;
                }
            }
            normalize_blue_fang_rotation_quaternion(rotation_quaternion).map(
                |normalized_quaternion| {
                    (
                        keyframe_time_milliseconds,
                        normalized_quaternion,
                        tension_continuity_bias,
                    )
                },
            )
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    Ok(BlueFangAnimationTrack::Rotation(
        BlueFangQuaternionCurve::new(keyframes, includes_control_values),
    ))
}

pub(super) fn parse_blue_fang_scale_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let keyframes = (0..keyframe_count)
        .map(|_| {
            let keyframe_time_milliseconds = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
            let uniform_scale =
                f32::from(read_blue_fang_u8(source_bytes, source_byte_cursor)?) / 50.0;
            let (incoming, outgoing) = if includes_control_values {
                (
                    f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0,
                    f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0,
                )
            } else {
                (0.0, 0.0)
            };
            Ok((
                keyframe_time_milliseconds,
                [uniform_scale; 3],
                [incoming; 3],
                [outgoing; 3],
            ))
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    Ok(BlueFangAnimationTrack::Scale(BlueFangScalarCurve::new(
        keyframes,
        includes_control_values,
    )))
}
