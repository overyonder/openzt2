//! Typed failures from NetImmerse NIF source parsing and validation.

use super::super::native_source_byte_reading_error::NativeSourceByteReadingError;

#[derive(Debug)]
pub(in super::super) enum NetImmerseNifSourceError {
    UnsupportedBlockType {
        source_path: String,
        block_type_name: String,
    },
    InvalidUtf8 {
        source_path: String,
        detail: String,
    },
    InvalidData {
        source_path: String,
        detail: String,
    },
}

impl NetImmerseNifSourceError {
    pub(super) fn unsupported_block_type(source_path: &str, block_type_name: &str) -> Self {
        Self::UnsupportedBlockType {
            source_path: source_path.to_owned(),
            block_type_name: block_type_name.to_owned(),
        }
    }

    pub(super) fn invalid_utf8(source_path: &str, detail: impl Into<String>) -> Self {
        Self::InvalidUtf8 {
            source_path: source_path.to_owned(),
            detail: detail.into(),
        }
    }

    pub(super) fn invalid_data(source_path: &str, detail: impl Into<String>) -> Self {
        Self::InvalidData {
            source_path: source_path.to_owned(),
            detail: detail.into(),
        }
    }

    pub(super) fn with_block_context(self, block_index: u32, block_type_name: &str) -> Self {
        let add_context =
            |detail: String| format!("block {block_index} {block_type_name}: {detail}");
        match self {
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
            error @ Self::UnsupportedBlockType { .. } => error,
        }
    }
}

impl From<NativeSourceByteReadingError> for NetImmerseNifSourceError {
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

impl std::fmt::Display for NetImmerseNifSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedBlockType {
                source_path,
                block_type_name,
            } => write!(
                formatter,
                "{source_path}: unsupported NIF block type {block_type_name}"
            ),
            Self::InvalidUtf8 {
                source_path,
                detail,
            } => write!(formatter, "{source_path}: invalid UTF-8: {detail}"),
            Self::InvalidData {
                source_path,
                detail,
            } => write!(formatter, "{source_path}: invalid NIF data: {detail}"),
        }
    }
}

impl std::error::Error for NetImmerseNifSourceError {}
