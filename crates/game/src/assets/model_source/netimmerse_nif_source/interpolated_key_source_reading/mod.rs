//! Interpolated key and key-group reading for NIF animation records.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_four_component_colour, read_quaternion,
        read_three_component_vector, read_u32_little_endian,
    },
    interpolated_key_source_types::{
        NetImmerseColorKey, NetImmerseColorKeyGroup, NetImmerseFloatKey, NetImmerseFloatKeyGroup,
        NetImmerseQuaternionKey, NetImmerseVector3Key, NetImmerseVector3KeyGroup,
    },
    source_error::NetImmerseNifSourceError,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

fn read_tension_bias_continuity(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[f32; 3]> {
    Ok([
        read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
        read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
        read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
    ])
}

pub(super) fn read_float_keys(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    key_count: u32,
    interpolation: u32,
    field_name: &str,
) -> Result<Vec<NetImmerseFloatKey>> {
    (0..key_count)
        .map(|_| {
            let time = read_f32_little_endian(source_bytes, cursor, source_path, field_name)?;
            let value = read_f32_little_endian(source_bytes, cursor, source_path, field_name)?;
            let (forward, backward, tbc) = if interpolation == 2 {
                (
                    Some(read_f32_little_endian(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    Some(read_f32_little_endian(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    None,
                )
            } else {
                let tbc = (interpolation == 3)
                    .then(|| {
                        read_tension_bias_continuity(source_bytes, cursor, source_path, field_name)
                    })
                    .transpose()?;
                (None, None, tbc)
            };
            Ok(NetImmerseFloatKey {
                time,
                value,
                forward,
                backward,
                tbc,
            })
        })
        .collect()
}

pub(super) fn read_quaternion_keys(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    key_count: u32,
    interpolation: u32,
    field_name: &str,
) -> Result<Vec<NetImmerseQuaternionKey>> {
    (0..key_count)
        .map(|_| {
            let time = read_f32_little_endian(source_bytes, cursor, source_path, field_name)?;
            let value = read_quaternion(source_bytes, cursor, source_path, field_name)?;
            let (forward, backward, tbc) = if interpolation == 2 {
                (
                    Some(read_quaternion(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    Some(read_quaternion(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    None,
                )
            } else {
                let tbc = (interpolation == 3)
                    .then(|| {
                        read_tension_bias_continuity(source_bytes, cursor, source_path, field_name)
                    })
                    .transpose()?;
                (None, None, tbc)
            };
            Ok(NetImmerseQuaternionKey {
                time,
                value,
                forward,
                backward,
                tbc,
            })
        })
        .collect()
}

pub(super) fn read_vector3_key_group(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseVector3KeyGroup> {
    let key_count = read_u32_little_endian(source_bytes, cursor, source_path, field_name)?;
    let interpolation = (key_count != 0)
        .then(|| read_u32_little_endian(source_bytes, cursor, source_path, field_name))
        .transpose()?;
    let keys = (0..key_count)
        .map(|_| {
            let time = read_f32_little_endian(source_bytes, cursor, source_path, field_name)?;
            let value = read_three_component_vector(source_bytes, cursor, source_path, field_name)?;
            let (forward, backward, tbc) = if interpolation == Some(2) {
                (
                    Some(read_three_component_vector(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    Some(read_three_component_vector(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    None,
                )
            } else {
                let tbc = (interpolation == Some(3))
                    .then(|| {
                        read_tension_bias_continuity(source_bytes, cursor, source_path, field_name)
                    })
                    .transpose()?;
                (None, None, tbc)
            };
            Ok(NetImmerseVector3Key {
                time,
                value,
                forward,
                backward,
                tbc,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseVector3KeyGroup {
        interpolation,
        keys,
    })
}

pub(super) fn read_colour_key_group(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseColorKeyGroup> {
    let key_count = read_u32_little_endian(source_bytes, cursor, source_path, field_name)?;
    let interpolation = (key_count != 0)
        .then(|| read_u32_little_endian(source_bytes, cursor, source_path, field_name))
        .transpose()?;
    let keys = (0..key_count)
        .map(|_| {
            let time = read_f32_little_endian(source_bytes, cursor, source_path, field_name)?;
            let value = read_four_component_colour(source_bytes, cursor, source_path, field_name)?;
            let (forward, backward) = if interpolation == Some(2) {
                (
                    Some(read_four_component_colour(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                    Some(read_four_component_colour(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?),
                )
            } else {
                if interpolation == Some(3) {
                    read_tension_bias_continuity(source_bytes, cursor, source_path, field_name)?;
                }
                (None, None)
            };
            Ok(NetImmerseColorKey {
                time,
                value,
                forward,
                backward,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseColorKeyGroup {
        interpolation,
        keys,
    })
}

pub(super) fn read_float_key_group(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseFloatKeyGroup> {
    let key_count = read_u32_little_endian(source_bytes, cursor, source_path, field_name)?;
    let interpolation = (key_count != 0)
        .then(|| read_u32_little_endian(source_bytes, cursor, source_path, field_name))
        .transpose()?;
    let keys = interpolation
        .map(|interpolation| {
            read_float_keys(
                source_bytes,
                cursor,
                source_path,
                key_count,
                interpolation,
                field_name,
            )
        })
        .transpose()?
        .unwrap_or_default();
    Ok(NetImmerseFloatKeyGroup {
        interpolation,
        keys,
    })
}
