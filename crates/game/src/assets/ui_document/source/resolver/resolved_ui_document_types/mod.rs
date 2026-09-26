//! Canonical lowering inputs produced by selected UI source resolution.

use super::super::lower::authored_ui_document_lowering::AuthoredUiDocument;

#[derive(Clone, Debug, Default)]
pub(in crate::assets::ui_document::source) struct ResolvedUiRoleAndFragmentDocuments {
    pub(in crate::assets::ui_document::source) roles: Vec<AuthoredUiDocument>,
    pub(in crate::assets::ui_document::source) fragments: Vec<AuthoredUiDocument>,
}
