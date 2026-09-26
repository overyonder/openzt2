use std::fmt;

use super::path::AssetPath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlueFangSourceDocumentParsingError {
    UnsupportedPath { path: AssetPath },
    InvalidText { path: AssetPath, message: String },
    InvalidMarkup { path: AssetPath, message: String },
}

impl fmt::Display for BlueFangSourceDocumentParsingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPath { path } => {
                write!(formatter, "unsupported document path {path:?}")
            }
            Self::InvalidText { path, message } => {
                write!(formatter, "invalid source text in {path:?}: {message}")
            }
            Self::InvalidMarkup { path, message } => {
                write!(formatter, "invalid source markup in {path:?}: {message}")
            }
        }
    }
}

impl std::error::Error for BlueFangSourceDocumentParsingError {}
