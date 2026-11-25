//! Blue Fang UI source parsing after the observed authored-widget repairs.

use crate::source_document::{
    ordered_source_document_types::OrderedSourceDocument,
    ui::{model::SourceUiWidgetKind, parser::SourceUiDocument},
};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    authored_ui_widget_source_normalization::normalize_observed_authored_ui_widget_source_shapes,
};

pub(super) fn parse_and_normalize_blue_fang_ui_source_document(
    document: &OrderedSourceDocument,
) -> Result<SourceUiDocument, UiSourceDocumentGap> {
    let normalized = normalize_observed_authored_ui_widget_source_shapes(document)?;
    let document = &normalized;
    if let Some(source) = SourceUiDocument::parse_hotkey_modes(document) {
        return Ok(source);
    }
    if !matches!(
        SourceUiWidgetKind::from_tag(&document.root.name),
        SourceUiWidgetKind::Unknown(_)
    ) {
        return Ok(parse_normalized_blue_fang_ui_source_document(document));
    }

    // ZTApp owns process bootstrap facts but embeds its authored UIRoot as a
    // direct child. Interpret that subtree without renaming it or serializing a
    // replacement document. Other embedded UI fragments remain with their
    // owning source family until resolution supplies an explicit UI lowerer role.
    let mut roots = document
        .root
        .element_children()
        .filter(|node| matches!(node.name.as_str(), "UIRoot" | "UIApp"));
    let Some(root) = roots.next() else {
        return Err(UiSourceDocumentGap::at_root(
            document,
            UiSourceDocumentFamily::Ui,
            UiSourceDocumentGapKind::WrongDocumentFamily,
            format!(
                "source root <{}> is not a supported Blue Fang UI document",
                document.root.name
            ),
        ));
    };
    if roots.next().is_some() {
        return Err(UiSourceDocumentGap::at_root(
            document,
            UiSourceDocumentFamily::Ui,
            UiSourceDocumentGapKind::UnsupportedVocabulary,
            "source document contains multiple top-level UI roots; UI lowerer needs an evidenced role/selection rule",
        ));
    }

    let subtree = OrderedSourceDocument {
        path: document.path.clone(),
        format: document.format,
        root: root.clone(),
    };
    Ok(parse_normalized_blue_fang_ui_source_document(&subtree))
}

fn parse_normalized_blue_fang_ui_source_document(
    document: &OrderedSourceDocument,
) -> SourceUiDocument {
    SourceUiDocument::parse(document)
}
