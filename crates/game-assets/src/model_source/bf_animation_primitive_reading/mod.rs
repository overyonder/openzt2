//! Bounds-checked primitive reads from Blue Fang animation source bytes.

use super::conversion_error::ConversionError;

pub(super) fn read_blue_fang_fixed_width_name(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
    field_byte_count: usize,
) -> Result<String, ConversionError> {
    let fixed_width_name_bytes =
        read_blue_fang_source_byte_range(source_bytes, source_byte_cursor, field_byte_count)?;
    let name_end_byte_offset = fixed_width_name_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(fixed_width_name_bytes.len());
    std::str::from_utf8(&fixed_width_name_bytes[..name_end_byte_offset])
        .map(str::trim_end)
        .map(str::to_owned)
        .map_err(|_| ConversionError::InvalidSource("non-UTF-8 BF node name"))
}

pub(super) fn read_blue_fang_u8(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<u8, ConversionError> {
    Ok(read_blue_fang_source_byte_range(source_bytes, source_byte_cursor, 1)?[0])
}

pub(super) fn read_blue_fang_u16(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<u16, ConversionError> {
    read_blue_fang_source_byte_range(source_bytes, source_byte_cursor, 2)?
        .try_into()
        .map(u16::from_le_bytes)
        .map_err(|_| ConversionError::InvalidSource("invalid BF u16 width"))
}

pub(super) fn read_blue_fang_i16(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<i16, ConversionError> {
    read_blue_fang_source_byte_range(source_bytes, source_byte_cursor, 2)?
        .try_into()
        .map(i16::from_le_bytes)
        .map_err(|_| ConversionError::InvalidSource("invalid BF i16 width"))
}

pub(super) fn read_blue_fang_u32(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<u32, ConversionError> {
    read_blue_fang_source_byte_range(source_bytes, source_byte_cursor, 4)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| ConversionError::InvalidSource("invalid BF u32 width"))
}

pub(super) fn read_blue_fang_f32(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<f32, ConversionError> {
    read_blue_fang_source_byte_range(source_bytes, source_byte_cursor, 4)?
        .try_into()
        .map(f32::from_le_bytes)
        .map_err(|_| ConversionError::InvalidSource("invalid BF f32 width"))
}

pub(super) fn read_blue_fang_source_byte_range<'a>(
    source_bytes: &'a [u8],
    source_byte_cursor: &mut usize,
    byte_count: usize,
) -> Result<&'a [u8], ConversionError> {
    let end_byte_offset = source_byte_cursor
        .checked_add(byte_count)
        .ok_or(ConversionError::InvalidSource("BF cursor overflow"))?;
    let source_byte_range = source_bytes
        .get(*source_byte_cursor..end_byte_offset)
        .ok_or(ConversionError::InvalidSource("truncated BF file"))?;
    *source_byte_cursor = end_byte_offset;
    Ok(source_byte_range)
}
