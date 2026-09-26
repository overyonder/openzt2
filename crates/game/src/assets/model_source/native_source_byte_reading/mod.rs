//! Bounds-checked primitive reading shared by native binary source families.

use super::native_source_byte_reading_error::NativeSourceByteReadingError;

type Result<T> = std::result::Result<T, NativeSourceByteReadingError>;

pub(super) fn read_sized_string(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<String> {
    let byte_length = read_u32_little_endian(bytes, cursor, source_path, field_name)?;
    let byte_length = usize::try_from(byte_length).map_err(|_| {
        NativeSourceByteReadingError::invalid_data(
            source_path,
            format!("{field_name} length does not fit usize"),
        )
    })?;
    let end = cursor.checked_add(byte_length).ok_or_else(|| {
        NativeSourceByteReadingError::invalid_data(
            source_path,
            format!("{field_name} length overflows file cursor"),
        )
    })?;
    let raw = bytes.get(*cursor..end).ok_or_else(|| {
        NativeSourceByteReadingError::invalid_data(source_path, format!("truncated {field_name}"))
    })?;
    *cursor = end;
    std::str::from_utf8(raw)
        .map(str::to_owned)
        .map_err(|error| {
            NativeSourceByteReadingError::invalid_utf8(
                source_path,
                format!("{field_name}: {error}"),
            )
        })
}

pub(super) fn read_source_boolean(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<bool> {
    read_u8(bytes, cursor, source_path, field_name).map(|value| value != 0)
}

pub(super) fn read_u8(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<u8> {
    let raw = bytes.get(*cursor).copied().ok_or_else(|| {
        NativeSourceByteReadingError::invalid_data(source_path, format!("truncated {field_name}"))
    })?;
    *cursor = (*cursor).saturating_add(1);
    Ok(raw)
}

pub(super) fn read_fixed_size_byte_array<const BYTE_COUNT: usize>(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[u8; BYTE_COUNT]> {
    let end = cursor.checked_add(BYTE_COUNT).ok_or_else(|| {
        NativeSourceByteReadingError::invalid_data(
            source_path,
            format!("{field_name} cursor overflow"),
        )
    })?;
    let raw = bytes.get(*cursor..end).ok_or_else(|| {
        NativeSourceByteReadingError::invalid_data(source_path, format!("truncated {field_name}"))
    })?;
    *cursor = end;
    raw.try_into().map_err(|_| {
        NativeSourceByteReadingError::invalid_data(source_path, format!("truncated {field_name}"))
    })
}

pub(super) fn read_u16_little_endian(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<u16> {
    read_fixed_size_byte_array(bytes, cursor, source_path, field_name).map(u16::from_le_bytes)
}

pub(super) fn read_i16_little_endian(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<i16> {
    read_fixed_size_byte_array(bytes, cursor, source_path, field_name).map(i16::from_le_bytes)
}

pub(super) fn read_u32_little_endian(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<u32> {
    read_fixed_size_byte_array(bytes, cursor, source_path, field_name).map(u32::from_le_bytes)
}

pub(super) fn read_i32_little_endian(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<i32> {
    read_fixed_size_byte_array(bytes, cursor, source_path, field_name).map(i32::from_le_bytes)
}

pub(super) fn read_f32_little_endian(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<f32> {
    read_fixed_size_byte_array(bytes, cursor, source_path, field_name).map(f32::from_le_bytes)
}

fn read_f32_array_with_fixed_length<const VALUE_COUNT: usize>(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[f32; VALUE_COUNT]> {
    let mut values = [0.0; VALUE_COUNT];
    for value in &mut values {
        *value = read_f32_little_endian(bytes, cursor, source_path, field_name)?;
    }
    Ok(values)
}

pub(super) fn read_three_component_vector(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[f32; 3]> {
    read_f32_array_with_fixed_length(bytes, cursor, source_path, field_name)
}

pub(super) fn read_three_by_three_matrix(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[f32; 9]> {
    read_f32_array_with_fixed_length(bytes, cursor, source_path, field_name)
}

pub(super) fn read_quaternion(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[f32; 4]> {
    read_f32_array_with_fixed_length(bytes, cursor, source_path, field_name)
}

pub(super) fn read_four_component_colour(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<[f32; 4]> {
    read_f32_array_with_fixed_length(bytes, cursor, source_path, field_name)
}

pub(super) fn read_f32_values(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    value_count: u16,
    field_name: &str,
) -> Result<Vec<f32>> {
    (0..usize::from(value_count))
        .map(|_| read_f32_little_endian(bytes, cursor, source_path, field_name))
        .collect()
}

pub(super) fn read_three_component_vectors(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    vector_count: u16,
    field_name: &str,
) -> Result<Vec<[f32; 3]>> {
    (0..usize::from(vector_count))
        .map(|_| read_three_component_vector(bytes, cursor, source_path, field_name))
        .collect()
}

pub(super) fn read_u32_counted_three_component_vectors(
    bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    vector_count: u32,
    field_name: &str,
) -> Result<Vec<[f32; 3]>> {
    usize::try_from(vector_count)
        .map_err(|_| {
            NativeSourceByteReadingError::invalid_data(
                source_path,
                format!("{field_name} count does not fit usize"),
            )
        })
        .and_then(|vector_count| {
            (0..vector_count)
                .map(|_| read_three_component_vector(bytes, cursor, source_path, field_name))
                .collect()
        })
}
