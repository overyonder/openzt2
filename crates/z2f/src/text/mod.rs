//! Blue Fang source-text data and procedures.

pub mod d3d9_effect_syntax;
pub mod decoding;
pub mod xml_like_document_syntax;

/// Encoding accepted by the live source-text boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEncoding {
    Utf8,
    Utf16LittleEndian,
    Utf16BigEndian,
}
