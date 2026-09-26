//! Typed bounds and UTF-8 failures from native model source byte reading.

#[derive(Debug)]
pub(super) enum NativeSourceByteReadingError {
    InvalidUtf8 { source_path: String, detail: String },
    InvalidData { source_path: String, detail: String },
}

impl NativeSourceByteReadingError {
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
}

impl std::fmt::Display for NativeSourceByteReadingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUtf8 {
                source_path,
                detail,
            } => write!(formatter, "{source_path}: invalid UTF-8: {detail}"),
            Self::InvalidData {
                source_path,
                detail,
            } => write!(formatter, "{source_path}: invalid model data: {detail}"),
        }
    }
}

impl std::error::Error for NativeSourceByteReadingError {}
