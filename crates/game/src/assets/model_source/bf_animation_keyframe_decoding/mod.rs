//! Blue Fang translation, rotation, scale, and scalar-axis keyframe decoding.

use super::{
    bf_animation_keyframe_interpolation::normalize_blue_fang_rotation_quaternion,
    bf_animation_primitive_reading::{read_blue_fang_i16, read_blue_fang_u16, read_blue_fang_u8},
    bf_animation_source_types::{BlueFangAnimationProperty, BlueFangAnimationTrack},
    conversion_error::ConversionError,
};

pub(super) fn parse_blue_fang_translation_vector_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let mut keyframes = (0..keyframe_count)
        .map(|_| {
            let keyframe_time_milliseconds = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
            let translation_components = [
                read_blue_fang_i16(source_bytes, source_byte_cursor)?,
                read_blue_fang_i16(source_bytes, source_byte_cursor)?,
                read_blue_fang_i16(source_bytes, source_byte_cursor)?,
            ]
            .map(|encoded_component| f32::from(encoded_component) / 1000.0);
            if includes_control_values {
                (0..6).try_for_each(|_| {
                    read_blue_fang_i16(source_bytes, source_byte_cursor).map(|_| ())
                })?;
            }
            Ok((keyframe_time_milliseconds, translation_components))
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    keyframes.sort_by_key(|keyframe| keyframe.0);
    keyframes.dedup_by_key(|keyframe| keyframe.0);
    Ok(BlueFangAnimationTrack {
        animated_property: BlueFangAnimationProperty::Translation,
        keyframe_times_seconds: keyframes
            .iter()
            .map(|keyframe| f32::from(keyframe.0) / 1000.0)
            .collect(),
        keyframe_values: keyframes
            .into_iter()
            .flat_map(|keyframe| keyframe.1)
            .collect(),
    })
}

pub(super) fn parse_blue_fang_scalar_axis_keyframes(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<Vec<(u16, f32)>, ConversionError> {
    let mut keyframes = (0..keyframe_count)
        .map(|_| {
            let scalar_axis_keyframe = (
                read_blue_fang_u16(source_bytes, source_byte_cursor)?,
                f32::from(read_blue_fang_i16(source_bytes, source_byte_cursor)?) / 1000.0,
            );
            if includes_control_values {
                read_blue_fang_i16(source_bytes, source_byte_cursor)?;
                read_blue_fang_i16(source_bytes, source_byte_cursor)?;
            }
            Ok(scalar_axis_keyframe)
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    keyframes.sort_by_key(|keyframe| keyframe.0);
    keyframes.dedup_by_key(|keyframe| keyframe.0);
    Ok(keyframes)
}

pub(super) fn parse_blue_fang_quaternion_rotation_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let mut keyframes = (0..keyframe_count)
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
            if includes_control_values {
                (0..3).try_for_each(|_| {
                    read_blue_fang_i16(source_bytes, source_byte_cursor).map(|_| ())
                })?;
            }
            normalize_blue_fang_rotation_quaternion(rotation_quaternion)
                .map(|normalized_quaternion| (keyframe_time_milliseconds, normalized_quaternion))
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    keyframes.sort_by_key(|keyframe| keyframe.0);
    keyframes.dedup_by_key(|keyframe| keyframe.0);
    Ok(BlueFangAnimationTrack {
        animated_property: BlueFangAnimationProperty::Rotation,
        keyframe_times_seconds: keyframes
            .iter()
            .map(|keyframe| f32::from(keyframe.0) / 1000.0)
            .collect(),
        keyframe_values: keyframes
            .into_iter()
            .flat_map(|keyframe| keyframe.1)
            .collect(),
    })
}

pub(super) fn parse_blue_fang_scale_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    keyframe_count: usize,
    includes_control_values: bool,
) -> Result<BlueFangAnimationTrack, ConversionError> {
    let mut keyframes = (0..keyframe_count)
        .map(|_| {
            let keyframe_time_milliseconds = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
            let uniform_scale =
                f32::from(read_blue_fang_u8(source_bytes, source_byte_cursor)?) / 50.0;
            if includes_control_values {
                read_blue_fang_i16(source_bytes, source_byte_cursor)?;
                read_blue_fang_i16(source_bytes, source_byte_cursor)?;
            }
            Ok((keyframe_time_milliseconds, [uniform_scale; 3]))
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    keyframes.sort_by_key(|keyframe| keyframe.0);
    keyframes.dedup_by_key(|keyframe| keyframe.0);
    Ok(BlueFangAnimationTrack {
        animated_property: BlueFangAnimationProperty::Scale,
        keyframe_times_seconds: keyframes
            .iter()
            .map(|keyframe| f32::from(keyframe.0) / 1000.0)
            .collect(),
        keyframe_values: keyframes
            .into_iter()
            .flat_map(|keyframe| keyframe.1)
            .collect(),
    })
}
