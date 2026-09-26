//! Conversion of transient source-parser diagnostics into UI resolution gaps.

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentSpan,
    ui::parser::{SourceUiDiagnostic, SourceUiDocument},
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

pub(super) fn convert_source_ui_diagnostic_to_document_gap(
    source_ui_document: &SourceUiDocument,
    source_ui_diagnostic: &SourceUiDiagnostic,
) -> UiSourceDocumentGap {
    let (diagnostic_span, diagnostic_message) = match source_ui_diagnostic {
        SourceUiDiagnostic::UnknownWidget { tag, span } => (
            *span,
            format!("UI lowerer does not support <{tag}>"),
        ),
        SourceUiDiagnostic::UnknownAttribute {
            widget,
            attribute,
            span,
        } => (
            *span,
            format!(
                "UI lowerer has no typed meaning for visible/behavior-bearing {widget} attribute {}={:?}",
                attribute.name, attribute.value
            ),
        ),
        SourceUiDiagnostic::UnknownElement {
            widget,
            element,
            span,
        } => (
            *span,
            format!("UI lowerer has no typed meaning for {widget} child element <{element}>"),
        ),
        SourceUiDiagnostic::InvalidValue {
            widget,
            field,
            value,
            span,
        } => (
            *span,
            format!("UI lowerer cannot interpret {widget} field {field}={value:?}"),
        ),
    };

    UiSourceDocumentGap {
        family: UiSourceDocumentFamily::Ui,
        kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
        virtual_path: source_ui_document.path.key(),
        span: nonempty_source_ui_diagnostic_span(diagnostic_span, source_ui_document.root.span),
        message: diagnostic_message,
    }
}

fn nonempty_source_ui_diagnostic_span(
    diagnostic_span: OrderedSourceDocumentSpan,
    source_root_span: OrderedSourceDocumentSpan,
) -> OrderedSourceDocumentSpan {
    if diagnostic_span.start < diagnostic_span.end {
        diagnostic_span
    } else {
        source_root_span
    }
}
