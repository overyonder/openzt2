//! Compound texture-slot and pixel-format reading for NIF render records.

use super::{
    super::native_source_byte_reading::{
        read_i16_little_endian, read_i32_little_endian, read_source_boolean, read_u32_little_endian,
    },
    counted_source_collection_reading::read_fixed_eight_source_bytes,
    render_property_and_texture_source_types::{NetImmerseNiPixelFormat, NetImmerseTextureSlot},
    source_error::NetImmerseNifSourceError,
    NETIMMERSE_VERSION_10_1_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn read_optional_texture_slot(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<Option<NetImmerseTextureSlot>> {
    if read_source_boolean(source_bytes, cursor, source_path, field_name)? {
        read_texture_slot(source_bytes, cursor, source_path, field_name).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn read_texture_slot(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseTextureSlot> {
    Ok(NetImmerseTextureSlot {
        source_ref: read_i32_little_endian(source_bytes, cursor, source_path, field_name)?,
        clamp_mode: read_u32_little_endian(source_bytes, cursor, source_path, field_name)?,
        filter_mode: read_u32_little_endian(source_bytes, cursor, source_path, field_name)?,
        uv_set: read_u32_little_endian(source_bytes, cursor, source_path, field_name)?,
        ps2_l: read_i16_little_endian(source_bytes, cursor, source_path, field_name)?,
        ps2_k: read_i16_little_endian(source_bytes, cursor, source_path, field_name)?,
    })
}

pub(super) fn read_pixel_format(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiPixelFormat> {
    let pixel_format =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiPixelFormat format")?;
    let red_mask =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiPixelFormat red mask")?;
    let green_mask = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiPixelFormat green mask",
    )?;
    let blue_mask =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiPixelFormat blue mask")?;
    let alpha_mask = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiPixelFormat alpha mask",
    )?;
    let bits_per_pixel = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiPixelFormat bits per pixel",
    )?;
    let old_fast_compare = read_fixed_eight_source_bytes(
        source_bytes,
        cursor,
        source_path,
        "NiPixelFormat fast compare",
    )?;
    let tiling = (encoded_version >= NETIMMERSE_VERSION_10_1_0_0)
        .then(|| read_u32_little_endian(source_bytes, cursor, source_path, "NiPixelFormat tiling"))
        .transpose()?;
    Ok(NetImmerseNiPixelFormat {
        pixel_format,
        red_mask,
        green_mask,
        blue_mask,
        alpha_mask,
        bits_per_pixel,
        old_fast_compare,
        tiling,
    })
}
