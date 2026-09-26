//! Bounds-checked counted collection reading shared by NIF record families.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_fixed_size_byte_array, read_i32_little_endian,
        read_u16_little_endian, read_u8,
    },
    source_error::NetImmerseNifSourceError,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn read_triangle_index_triplets(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    triangle_count: u32,
    field_name: &str,
) -> Result<Vec<[u16; 3]>> {
    (0..triangle_count)
        .map(|_| {
            Ok([
                read_u16_little_endian(source_bytes, cursor, source_path, field_name)?,
                read_u16_little_endian(source_bytes, cursor, source_path, field_name)?,
                read_u16_little_endian(source_bytes, cursor, source_path, field_name)?,
            ])
        })
        .collect()
}

pub(super) fn read_u16_values(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    value_count: u32,
    field_name: &str,
) -> Result<Vec<u16>> {
    (0..value_count)
        .map(|_| {
            read_u16_little_endian(source_bytes, cursor, source_path, field_name)
                .map_err(Into::into)
        })
        .collect()
}

pub(super) fn read_i32_values(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    value_count: u32,
    field_name: &str,
) -> Result<Vec<i32>> {
    (0..value_count)
        .map(|_| {
            read_i32_little_endian(source_bytes, cursor, source_path, field_name)
                .map_err(Into::into)
        })
        .collect()
}

pub(super) fn read_raw_source_bytes(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    byte_count: u32,
    field_name: &str,
) -> Result<Vec<u8>> {
    let byte_count = usize::try_from(byte_count).map_err(|_| {
        NetImmerseNifSourceError::invalid_data(
            source_path,
            format!("{field_name} length does not fit usize"),
        )
    })?;
    let end = cursor.checked_add(byte_count).ok_or_else(|| {
        NetImmerseNifSourceError::invalid_data(source_path, format!("{field_name} cursor overflow"))
    })?;
    let raw_source_bytes = source_bytes.get(*cursor..end).ok_or_else(|| {
        NetImmerseNifSourceError::invalid_data(source_path, format!("truncated {field_name}"))
    })?;
    *cursor = end;
    Ok(raw_source_bytes.to_vec())
}

pub(super) fn read_fixed_eight_source_bytes(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[u8; 8]> {
    read_fixed_size_byte_array(source_bytes, cursor, source_path, field_name).map_err(Into::into)
}

pub(super) fn read_f32_value_grid(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    row_count: u32,
    column_count: u32,
    field_name: &str,
) -> Result<Vec<Vec<f32>>> {
    (0..row_count)
        .map(|_| {
            (0..column_count)
                .map(|_| {
                    read_f32_little_endian(source_bytes, cursor, source_path, field_name)
                        .map_err(Into::into)
                })
                .collect()
        })
        .collect()
}

pub(super) fn read_u8_value_grid(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    row_count: u32,
    column_count: u32,
    field_name: &str,
) -> Result<Vec<Vec<u8>>> {
    (0..row_count)
        .map(|_| {
            (0..column_count)
                .map(|_| read_u8(source_bytes, cursor, source_path, field_name).map_err(Into::into))
                .collect()
        })
        .collect()
}
