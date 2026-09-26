//! Typed failures from Blue Fang BFB source parsing and validation.

use super::super::native_source_byte_reading_error::NativeSourceByteReadingError;

#[derive(Debug)]
pub(in super::super) enum BlueFangBfbSourceError {
    UnsupportedFormat { source_path: String },
    InvalidUtf8 { source_path: String, detail: String },
    InvalidData { source_path: String, detail: String },
}

impl BlueFangBfbSourceError {
    pub(super) fn unsupported_format(source_path: String) -> Self {
        Self::UnsupportedFormat { source_path }
    }

    pub(super) fn invalid_data(source_path: &str, detail: impl Into<String>) -> Self {
        Self::InvalidData {
            source_path: source_path.to_owned(),
            detail: detail.into(),
        }
    }

    pub(super) fn with_object_block_context(self, object_block_index: u32) -> Self {
        let add_context =
            |detail: String| format!("BFB object block {object_block_index}: {detail}");
        match self {
            Self::UnsupportedFormat { source_path } => Self::UnsupportedFormat { source_path },
            Self::InvalidUtf8 {
                source_path,
                detail,
            } => Self::InvalidUtf8 {
                source_path,
                detail: add_context(detail),
            },
            Self::InvalidData {
                source_path,
                detail,
            } => Self::InvalidData {
                source_path,
                detail: add_context(detail),
            },
        }
    }
}

impl From<NativeSourceByteReadingError> for BlueFangBfbSourceError {
    fn from(error: NativeSourceByteReadingError) -> Self {
        match error {
            NativeSourceByteReadingError::InvalidUtf8 {
                source_path,
                detail,
            } => Self::InvalidUtf8 {
                source_path,
                detail,
            },
            NativeSourceByteReadingError::InvalidData {
                source_path,
                detail,
            } => Self::InvalidData {
                source_path,
                detail,
            },
        }
    }
}

impl std::fmt::Display for BlueFangBfbSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat { source_path } => {
                write!(formatter, "{source_path}: unsupported BFB format")
            }
            Self::InvalidUtf8 {
                source_path,
                detail,
            } => write!(formatter, "{source_path}: invalid UTF-8: {detail}"),
            Self::InvalidData {
                source_path,
                detail,
            } => write!(formatter, "{source_path}: invalid BFB data: {detail}"),
        }
    }
}

impl std::error::Error for BlueFangBfbSourceError {}
