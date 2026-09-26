//! Typed failures produced while lowering native source scenes.

use std::fmt;

#[derive(Debug)]
pub(in super::super) struct NativeSceneLoweringError {
    source_path: String,
    detail: String,
}

impl NativeSceneLoweringError {
    pub(super) fn new(source_path: &str, detail: impl Into<String>) -> Self {
        Self {
            source_path: source_path.to_owned(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for NativeSceneLoweringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.source_path, self.detail)
    }
}

impl std::error::Error for NativeSceneLoweringError {}
