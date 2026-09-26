use std::io;

use super::SourceEncoding;

/// Decodes the observed UTF-8 and byte-order-marked UTF-16 source forms.
///
/// # Errors
///
/// Returns an error for invalid UTF-8, invalid UTF-16, or an incomplete UTF-16
/// code unit.
pub fn decode_blue_fang_source_text_from_utf8_or_utf16_bytes(
    bytes: &[u8],
) -> io::Result<(SourceEncoding, String)> {
    if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16(bytes, SourceEncoding::Utf16LittleEndian, u16::from_le_bytes);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16(bytes, SourceEncoding::Utf16BigEndian, u16::from_be_bytes);
    }
    std::str::from_utf8(bytes)
        .map(|text| {
            (
                SourceEncoding::Utf8,
                text.trim_start_matches('\u{feff}').to_owned(),
            )
        })
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn decode_utf16(
    bytes: &[u8],
    encoding: SourceEncoding,
    decode_unit: fn([u8; 2]) -> u16,
) -> io::Result<(SourceEncoding, String)> {
    if !bytes.len().is_multiple_of(2) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "odd byte count after UTF-16 byte-order mark",
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| decode_unit([pair[0], pair[1]]));
    String::from_utf16(&units.collect::<Vec<_>>())
        .map(|text| (encoding, text))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
