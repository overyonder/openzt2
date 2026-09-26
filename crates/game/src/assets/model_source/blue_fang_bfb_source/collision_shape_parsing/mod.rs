//! Blue Fang BFB oriented-box, sphere, and capsule record parsing.

use super::super::native_source_byte_reading::{read_f32_little_endian, read_u8};
use super::{
    object_block_framing::BlueFangBfbObjectBlock,
    source_error::BlueFangBfbSourceError,
    source_matrix_reading::read_blue_fang_bfb_row_major_matrix,
    source_types::{BlueFangBfbCapsule, BlueFangBfbOrientedBox, BlueFangBfbSphere},
};

type Result<T> = std::result::Result<T, BlueFangBfbSourceError>;

pub(super) fn parse_blue_fang_bfb_oriented_box(
    bytes: &[u8],
    object_block: &BlueFangBfbObjectBlock,
    source_path: &str,
) -> Result<BlueFangBfbOrientedBox> {
    const PAYLOAD_BYTE_COUNT: usize = 2 + 16 * 4 + 3 * 4;
    if object_block
        .block_end
        .saturating_sub(object_block.content_start)
        < PAYLOAD_BYTE_COUNT
    {
        return Err(BlueFangBfbSourceError::invalid_data(
            source_path,
            format!("truncated BFB oriented-box block {}", object_block.block_id),
        ));
    }
    let mut cursor = object_block.content_start;
    let flags = [
        read_u8(bytes, &mut cursor, source_path, "BFB oriented-box flag 0")?,
        read_u8(bytes, &mut cursor, source_path, "BFB oriented-box flag 1")?,
    ];
    let transform = read_blue_fang_bfb_row_major_matrix(
        bytes,
        &mut cursor,
        source_path,
        "BFB hierarchy transform",
    )?;
    let mut half_extents = [0.0; 3];
    for half_extent in &mut half_extents {
        *half_extent = read_f32_little_endian(
            bytes,
            &mut cursor,
            source_path,
            "BFB oriented-box half extent",
        )?;
    }
    Ok(BlueFangBfbOrientedBox {
        block_id: object_block.block_id,
        name: object_block.name.clone(),
        flags,
        transform,
        half_extents,
    })
}

pub(super) fn parse_blue_fang_bfb_sphere(
    bytes: &[u8],
    object_block: &BlueFangBfbObjectBlock,
    source_path: &str,
) -> Result<BlueFangBfbSphere> {
    const PAYLOAD_BYTE_COUNT: usize = 2 + 4 * 4;
    if object_block
        .block_end
        .saturating_sub(object_block.content_start)
        < PAYLOAD_BYTE_COUNT
    {
        return Err(BlueFangBfbSourceError::invalid_data(
            source_path,
            format!("truncated BFB sphere block {}", object_block.block_id),
        ));
    }
    let mut cursor = object_block.content_start;
    let flags = [
        read_u8(bytes, &mut cursor, source_path, "BFB sphere flag 0")?,
        read_u8(bytes, &mut cursor, source_path, "BFB sphere flag 1")?,
    ];
    let mut center = [0.0; 3];
    for component in &mut center {
        *component = read_f32_little_endian(bytes, &mut cursor, source_path, "BFB sphere center")?;
    }
    let radius = read_f32_little_endian(bytes, &mut cursor, source_path, "BFB sphere radius")?;
    Ok(BlueFangBfbSphere {
        block_id: object_block.block_id,
        name: object_block.name.clone(),
        flags,
        center,
        radius,
    })
}

pub(super) fn parse_blue_fang_bfb_capsule(
    bytes: &[u8],
    object_block: &BlueFangBfbObjectBlock,
    source_path: &str,
) -> Result<BlueFangBfbCapsule> {
    const PAYLOAD_BYTE_COUNT: usize = 2 + 7 * 4;
    if object_block
        .block_end
        .saturating_sub(object_block.content_start)
        < PAYLOAD_BYTE_COUNT
    {
        return Err(BlueFangBfbSourceError::invalid_data(
            source_path,
            format!("truncated BFB capsule block {}", object_block.block_id),
        ));
    }
    let mut cursor = object_block.content_start;
    let flags = [
        read_u8(bytes, &mut cursor, source_path, "BFB capsule flag 0")?,
        read_u8(bytes, &mut cursor, source_path, "BFB capsule flag 1")?,
    ];
    let mut start = [0.0; 3];
    for component in &mut start {
        *component = read_f32_little_endian(bytes, &mut cursor, source_path, "BFB capsule start")?;
    }
    let mut end = [0.0; 3];
    for component in &mut end {
        *component = read_f32_little_endian(bytes, &mut cursor, source_path, "BFB capsule end")?;
    }
    let radius = read_f32_little_endian(bytes, &mut cursor, source_path, "BFB capsule radius")?;
    Ok(BlueFangBfbCapsule {
        block_id: object_block.block_id,
        name: object_block.name.clone(),
        flags,
        start,
        end,
        radius,
    })
}
