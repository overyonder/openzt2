//! Exact row-major matrix reading from Blue Fang BFB source bytes.

use super::super::native_source_byte_reading::read_f32_little_endian;
use super::source_error::BlueFangBfbSourceError;

pub(super) fn read_blue_fang_bfb_row_major_matrix(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[[f32; 4]; 4], BlueFangBfbSourceError> {
    let mut matrix = [[0.0; 4]; 4];
    for row in &mut matrix {
        for value in row {
            *value = read_f32_little_endian(bytes, cursor, source_path, field_name)?;
        }
    }
    Ok(matrix)
}
