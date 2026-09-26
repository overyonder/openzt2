//! Source-document resolution gap schema.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UiSourceDocumentFamily {
    Ui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UiSourceDocumentGapKind {
    WrongDocumentFamily,
    UnsupportedVocabulary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UiSourceDocumentGap {
    pub(super) family: UiSourceDocumentFamily,
    pub(super) kind: UiSourceDocumentGapKind,
    pub(super) virtual_path: String,
    pub(super) span: OrderedSourceDocumentSpan,
    pub(super) message: String,
}

impl UiSourceDocumentGap {
    pub(super) fn at_root(
        document: &OrderedSourceDocument,
        family: UiSourceDocumentFamily,
        kind: UiSourceDocumentGapKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            family,
            kind,
            virtual_path: document.path.key(),
            span: document.root.span,
            message: message.into(),
        }
    }

    pub(super) fn at_node(
        document: &OrderedSourceDocument,
        node: &OrderedSourceDocumentNode,
        family: UiSourceDocumentFamily,
        kind: UiSourceDocumentGapKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            family,
            kind,
            virtual_path: document.path.key(),
            span: node.span,
            message: message.into(),
        }
    }
}
