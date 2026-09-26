use bevy::prelude::*;

/// Optional diagnostic override for authored image-selection groups.
///
/// The UI document remains the sole owner of candidate images and their
/// grouping. This primitive index only makes a particular native candidate
/// reproducible for screenshot comparison; normal game sessions choose once
/// when the projected screen instance is created.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredImageSelectionDiagnosticOverride {
    candidate_index: usize,
}

impl UiAuthoredImageSelectionDiagnosticOverride {
    pub(crate) fn from_candidate_index(candidate_index: usize) -> Self {
        Self { candidate_index }
    }

    pub(super) fn candidate_index(&self) -> usize {
        self.candidate_index
    }
}
