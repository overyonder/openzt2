//! Focused parsing of NIF colour, scalar, position, UV, and visibility data blocks.

use super::{
    super::native_source_byte_reading::{read_f32_little_endian, read_u32_little_endian, read_u8},
    interpolated_key_source_reading::{
        read_colour_key_group, read_float_key_group, read_vector3_key_group,
    },
    interpolated_key_source_types::{
        NetImmerseByteKey, NetImmerseNiColorData, NetImmerseNiFloatData, NetImmerseNiPosData,
        NetImmerseNiUvData, NetImmerseNiVisData,
    },
    source_error::NetImmerseNifSourceError,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_colour_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiColorData> {
    Ok(NetImmerseNiColorData {
        data: read_colour_key_group(source_bytes, cursor, source_path, "NiColorData keys")?,
    })
}

pub(super) fn parse_float_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiFloatData> {
    Ok(NetImmerseNiFloatData {
        data: read_float_key_group(source_bytes, cursor, source_path, "NiFloatData keys")?,
    })
}

pub(super) fn parse_position_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiPosData> {
    Ok(NetImmerseNiPosData {
        data: read_vector3_key_group(source_bytes, cursor, source_path, "NiPosData keys")?,
    })
}

pub(super) fn parse_uv_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiUvData> {
    let groups = (0..4)
        .map(|index| {
            read_float_key_group(
                source_bytes,
                cursor,
                source_path,
                &format!("NiUVData group {index}"),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiUvData { groups })
}

pub(super) fn parse_visibility_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiVisData> {
    let key_count =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiVisData key count")?;
    let keys = (0..key_count)
        .map(|_| {
            Ok(NetImmerseByteKey {
                time: read_f32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiVisData key time",
                )?,
                value: read_u8(source_bytes, cursor, source_path, "NiVisData key value")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiVisData { keys })
}
