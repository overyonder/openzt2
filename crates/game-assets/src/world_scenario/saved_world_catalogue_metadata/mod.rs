use quick_xml::{events::Event, Reader};
use std::{io, path::Path};

pub fn read_saved_world_display_name(bytes: &[u8]) -> io::Result<String> {
    let (_, text) =
        z2f::text::decoding::decode_blue_fang_source_text_from_utf8_or_utf16_bytes(bytes)?;
    let text =
        z2f::text::xml_like_document_syntax::repair_observed_blue_fang_xml_like_document_syntax(
            Path::new("saved/world.zt2"),
            &text,
        );
    let mut reader = Reader::from_str(&text);
    let mut depth = 0_usize;
    loop {
        match reader.read_event().map_err(io::Error::other)? {
            Event::Start(node) | Event::Empty(node)
                if depth == 2 && node.local_name().as_ref() == b"ZTStatus" =>
            {
                let name = node
                    .try_get_attribute(b"zooName")
                    .map_err(io::Error::other)?
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "saved zoo has no display name")
                    })?;
                let name = name
                    .decode_and_unescape_value(reader.decoder())
                    .map_err(io::Error::other)?
                    .into_owned();
                if name.is_empty() || name.chars().any(char::is_control) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "saved zoo display name is empty or contains control characters",
                    ));
                }
                return Ok(name);
            }
            Event::Start(node) => {
                if depth == 0 && node.local_name().as_ref() != b"saveRoot"
                    || depth == 1 && node.local_name().as_ref() != b"BFContext"
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "saved zoo has no save context",
                    ));
                }
                depth = depth.checked_add(1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "saved zoo nesting overflow")
                })?;
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "saved zoo nesting underflow")
                })?;
            }
            Event::Eof => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "saved zoo has no status record",
                ))
            }
            _ => {}
        }
    }
}
