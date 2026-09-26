//! Binary parsing and interpolation for Blue Fang `.bf` animation clips.

use super::{
    bf_animation_keyframe_decoding::{
        parse_blue_fang_quaternion_rotation_track, parse_blue_fang_scalar_axis_keyframes,
        parse_blue_fang_scale_track, parse_blue_fang_translation_vector_track,
    },
    bf_animation_keyframe_interpolation::{
        combine_blue_fang_euler_axes_into_quaternion_track,
        combine_blue_fang_scalar_axes_into_vector_track,
    },
    bf_animation_primitive_reading::{
        read_blue_fang_f32, read_blue_fang_fixed_width_name, read_blue_fang_source_byte_range,
        read_blue_fang_u16, read_blue_fang_u32,
    },
    bf_animation_source_types::{
        BlueFangAnimationMarker, BlueFangAnimationNode, BlueFangAnimationProperty,
        BlueFangAnimationTrack, ParsedBlueFangAnimation,
    },
    conversion_error::ConversionError,
};

const BLUE_FANG_ANIMATION_HEADER_BYTE_COUNT: usize = 12;
const BLUE_FANG_VERSION_1_FIRST_NODE_BYTE_OFFSET: usize = 128;
const BLUE_FANG_ANIMATION_NODE_HEADER_BYTE_COUNT: usize = 44;
const BLUE_FANG_VERSION_1_ANIMATION_NODE_HEADER_BYTE_COUNT: usize = 84;

pub(super) fn parse_blue_fang_animation_source_bytes(
    source_bytes: &[u8],
) -> Result<ParsedBlueFangAnimation, ConversionError> {
    let mut source_byte_cursor = 0;
    let animation_format_version = read_blue_fang_u32(source_bytes, &mut source_byte_cursor)?;
    let authored_duration_seconds = read_blue_fang_f32(source_bytes, &mut source_byte_cursor)?;
    let animation_node_count =
        usize::from(read_blue_fang_u16(source_bytes, &mut source_byte_cursor)?);
    let authored_animation_flags = read_blue_fang_u16(source_bytes, &mut source_byte_cursor)?;
    if !authored_duration_seconds.is_finite() || authored_duration_seconds < 0.0 {
        return Err(ConversionError::InvalidValue("invalid BF duration"));
    }
    source_byte_cursor = if animation_format_version == 1 {
        BLUE_FANG_VERSION_1_FIRST_NODE_BYTE_OFFSET
    } else {
        BLUE_FANG_ANIMATION_HEADER_BYTE_COUNT
    };
    if source_byte_cursor > source_bytes.len() {
        return Err(ConversionError::InvalidSource(
            "truncated BF animation header",
        ));
    }
    let animation_nodes = (0..animation_node_count)
        .map(|_| {
            parse_blue_fang_animation_node(
                source_bytes,
                &mut source_byte_cursor,
                animation_format_version,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let animation_markers =
        parse_blue_fang_animation_markers(source_bytes, &mut source_byte_cursor)?;
    let trailing_source_bytes = &source_bytes[source_byte_cursor..];
    if trailing_source_bytes != [0; 7] && trailing_source_bytes.iter().any(|byte| *byte != 0) {
        return Err(ConversionError::InvalidSource("unowned BF trailing bytes"));
    }
    let authored_duration_seconds = animation_nodes
        .iter()
        .flat_map(|node| &node.animation_tracks)
        .flat_map(|track| track.keyframe_times_seconds.last())
        .chain(
            animation_markers
                .iter()
                .map(|marker| &marker.event_time_seconds),
        )
        .copied()
        .fold(authored_duration_seconds, f32::max);
    Ok(ParsedBlueFangAnimation {
        animation_nodes,
        duration_seconds: authored_duration_seconds,
        authored_flags: authored_animation_flags,
        animation_markers,
    })
}

fn parse_blue_fang_animation_markers(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<Vec<BlueFangAnimationMarker>, ConversionError> {
    let mut animation_markers = Vec::new();
    loop {
        let Some(animation_marker_header_bytes) =
            source_bytes.get(*source_byte_cursor..(*source_byte_cursor).saturating_add(6))
        else {
            break;
        };
        let event_time_seconds = f32::from_le_bytes(
            animation_marker_header_bytes[..4]
                .try_into()
                .map_err(|_| ConversionError::InvalidSource("invalid BF marker time width"))?,
        );
        let marker_name_byte_count = usize::from(u16::from_le_bytes(
            animation_marker_header_bytes[4..]
                .try_into()
                .map_err(|_| ConversionError::InvalidSource("invalid BF marker name width"))?,
        ));
        if marker_name_byte_count == 0 {
            break;
        }
        *source_byte_cursor += 6;
        let marker_name_bytes = read_blue_fang_source_byte_range(
            source_bytes,
            source_byte_cursor,
            marker_name_byte_count,
        )?;
        let marker_name_end = marker_name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(marker_name_bytes.len());
        let marker_name = std::str::from_utf8(&marker_name_bytes[..marker_name_end])
            .map_err(|_| ConversionError::InvalidSource("non-UTF-8 BF marker name"))?
            .to_owned();
        if marker_name.is_empty() || !event_time_seconds.is_finite() || event_time_seconds < 0.0 {
            return Err(ConversionError::InvalidSource("invalid BF marker"));
        }
        animation_markers.push(BlueFangAnimationMarker {
            event_time_seconds,
            marker_name,
        });
    }
    Ok(animation_markers)
}

fn parse_blue_fang_animation_node(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    animation_format_version: u32,
) -> Result<BlueFangAnimationNode, ConversionError> {
    let animation_node_start_byte_offset = *source_byte_cursor;
    let skeleton_joint_name =
        read_blue_fang_fixed_width_name(source_bytes, source_byte_cursor, 32)?;
    let animation_track_count = usize::from(read_blue_fang_u16(source_bytes, source_byte_cursor)?);
    *source_byte_cursor = source_byte_cursor
        .checked_add(2)
        .ok_or(ConversionError::InvalidSource("BF node overflow"))?;
    let record_byte_length = usize::from(read_blue_fang_u16(source_bytes, source_byte_cursor)?);
    *source_byte_cursor = source_byte_cursor
        .checked_add(6)
        .ok_or(ConversionError::InvalidSource("BF node overflow"))?;
    let animation_node_end_byte_offset = animation_node_start_byte_offset
        .checked_add(record_byte_length)
        .filter(|end_byte_offset| *end_byte_offset <= source_bytes.len())
        .ok_or(ConversionError::InvalidSource("invalid BF node range"))?;
    *source_byte_cursor = animation_node_start_byte_offset
        .checked_add(if animation_format_version == 1 {
            BLUE_FANG_VERSION_1_ANIMATION_NODE_HEADER_BYTE_COUNT
        } else {
            BLUE_FANG_ANIMATION_NODE_HEADER_BYTE_COUNT
        })
        .filter(|node_data_byte_offset| *node_data_byte_offset <= animation_node_end_byte_offset)
        .ok_or(ConversionError::InvalidSource("invalid BF node header"))?;

    let mut animation_tracks = Vec::new();
    let mut translation_axis_keyframes: [Option<Vec<(u16, f32)>>; 3] = [None, None, None];
    let mut euler_axis_keyframes: [Option<Vec<(u16, f32)>>; 3] = [None, None, None];
    for _ in 0..animation_track_count {
        parse_blue_fang_animation_track(
            source_bytes,
            source_byte_cursor,
            animation_node_end_byte_offset,
            &mut animation_tracks,
            &mut translation_axis_keyframes,
            &mut euler_axis_keyframes,
        )?;
    }
    if translation_axis_keyframes.iter().any(Option::is_some) {
        animation_tracks.push(combine_blue_fang_scalar_axes_into_vector_track(
            &translation_axis_keyframes,
            BlueFangAnimationProperty::Translation,
        )?);
    }
    if euler_axis_keyframes.iter().any(Option::is_some) {
        animation_tracks.push(combine_blue_fang_euler_axes_into_quaternion_track(
            &euler_axis_keyframes,
        )?);
    }
    animation_tracks.sort_by_key(|track| track.animated_property);
    if animation_tracks
        .windows(2)
        .any(|pair| pair[0].animated_property == pair[1].animated_property)
    {
        return Err(ConversionError::InvalidSource(
            "multiple BF tracks target one node property",
        ));
    }
    *source_byte_cursor = animation_node_end_byte_offset;
    Ok(BlueFangAnimationNode {
        skeleton_joint_name,
        animation_tracks,
    })
}

fn parse_blue_fang_animation_track(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    animation_node_end_byte_offset: usize,
    animation_tracks: &mut Vec<BlueFangAnimationTrack>,
    translation_axis_keyframes: &mut [Option<Vec<(u16, f32)>>; 3],
    euler_axis_keyframes: &mut [Option<Vec<(u16, f32)>>; 3],
) -> Result<(), ConversionError> {
    let animation_track_start_byte_offset = *source_byte_cursor;
    let track_format_identifier = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
    let keyframe_count = usize::from(read_blue_fang_u16(source_bytes, source_byte_cursor)?);
    let record_byte_length = usize::from(read_blue_fang_u16(source_bytes, source_byte_cursor)?);
    let _reserved_track_header_value = read_blue_fang_u16(source_bytes, source_byte_cursor)?;
    let animation_track_end_byte_offset = animation_track_start_byte_offset
        .checked_add(record_byte_length)
        .filter(|end_byte_offset| *end_byte_offset <= animation_node_end_byte_offset)
        .ok_or(ConversionError::InvalidSource("invalid BF track range"))?;
    match track_format_identifier {
        1 | 2 => animation_tracks.push(parse_blue_fang_translation_vector_track(
            source_bytes,
            source_byte_cursor,
            keyframe_count,
            track_format_identifier == 1,
        )?),
        3..=5 => {
            translation_axis_keyframes[usize::from(track_format_identifier - 3)] =
                Some(parse_blue_fang_scalar_axis_keyframes(
                    source_bytes,
                    source_byte_cursor,
                    keyframe_count,
                    false,
                )?);
        }
        6..=8 => {
            euler_axis_keyframes[usize::from(track_format_identifier - 6)] =
                Some(parse_blue_fang_scalar_axis_keyframes(
                    source_bytes,
                    source_byte_cursor,
                    keyframe_count,
                    true,
                )?);
        }
        12 | 14 => animation_tracks.push(parse_blue_fang_quaternion_rotation_track(
            source_bytes,
            source_byte_cursor,
            keyframe_count,
            track_format_identifier == 12,
        )?),
        16 | 17 => animation_tracks.push(parse_blue_fang_scale_track(
            source_bytes,
            source_byte_cursor,
            keyframe_count,
            track_format_identifier == 16,
        )?),
        _ => return Err(ConversionError::InvalidSource("unsupported BF track")),
    }
    *source_byte_cursor = animation_track_end_byte_offset;
    Ok(())
}
