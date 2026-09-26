//! Blue Fang BFB object-block header framing and validation.

use super::super::native_source_byte_reading::{read_u16_little_endian, read_u32_little_endian};
use super::source_error::BlueFangBfbSourceError;

type Result<T> = std::result::Result<T, BlueFangBfbSourceError>;

pub(super) struct BlueFangBfbObjectBlock {
    pub(super) block_id: u32,
    pub(super) block_type: u16,
    pub(super) block_end: usize,
    pub(super) name: String,
    pub(super) content_start: usize,
}

pub(super) fn parse_blue_fang_bfb_object_block(
    bytes: &[u8],
    block_start: usize,
    source_path: &str,
) -> Result<BlueFangBfbObjectBlock> {
    let header_end = block_start.checked_add(76).ok_or_else(|| {
        BlueFangBfbSourceError::invalid_data(source_path, "BFB object block header offset overflow")
    })?;
    if header_end > bytes.len() {
        return Err(BlueFangBfbSourceError::invalid_data(
            source_path,
            "truncated BFB object block header",
        ));
    }
    let mut block_id_cursor = block_start;
    let block_id =
        read_u32_little_endian(bytes, &mut block_id_cursor, source_path, "BFB block ID")?;
    let mut block_type_cursor = block_start + 4;
    let block_type = read_u16_little_endian(
        bytes,
        &mut block_type_cursor,
        source_path,
        "BFB object block type",
    )?;
    let mut block_end_cursor = block_start + 8;
    let block_end =
        read_u32_little_endian(bytes, &mut block_end_cursor, source_path, "BFB block end")?;
    let block_end = usize::try_from(block_end).map_err(|_| {
        BlueFangBfbSourceError::invalid_data(source_path, "BFB block end does not fit usize")
    })?;
    if block_end <= header_end || block_end > bytes.len() {
        return Err(BlueFangBfbSourceError::invalid_data(
            source_path,
            format!(
                "BFB block end {block_end} does not follow header at {block_start} within file"
            ),
        ));
    }
    let name = parse_blue_fang_bfb_padded_string(
        &bytes[block_start + 12..header_end],
        source_path,
        "BFB object block name",
    )
    .or_else(|error| match block_type {
        5 | 8 => Ok(String::new()),
        _ => Err(error),
    })?;
    Ok(BlueFangBfbObjectBlock {
        block_id,
        block_type,
        block_end,
        name,
        content_start: header_end,
    })
}

pub(super) fn parse_blue_fang_bfb_padded_string(
    bytes: &[u8],
    source_path: &str,
    field_name: &str,
) -> Result<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end])
        .map(str::to_owned)
        .map_err(|error| BlueFangBfbSourceError::InvalidUtf8 {
            source_path: source_path.to_owned(),
            detail: format!("{field_name}: {error}"),
        })
}
