//! Blue Fang UI source parsing after the observed authored-widget repairs.

use crate::assets::source_document::{
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
    if let Some(mut source) = SourceUiDocument::parse_hotkey_modes(document) {
        remove_source_only_bindings(&mut source.root);
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
    let mut source = SourceUiDocument::parse(document);
    remove_source_only_bindings(&mut source.root);
    source
}

/// Bevy's input plugin owns engine-level held-key state directly. Authored
/// developer commands remain in the document and compile to native developer
/// actions; being hidden in the retail UI does not make them source-only.
fn remove_source_only_bindings(node: &mut crate::assets::source_document::ui::model::SourceUiNode) {
    node.hotkeys
        .retain(|hotkey| !source_only_event(&hotkey.event));
    node.events.iter_mut().for_each(|block| {
        block.events.retain(|event| !source_only_event(event));
    });
    node.children
        .iter_mut()
        .for_each(remove_source_only_bindings);
}

fn source_only_event(event: &crate::assets::source_document::ui::model::SourceUiEvent) -> bool {
    matches!(
        event.message.as_str(),
        "ZT_SET_COMMAND_STATE" | "ZT_CLEAR_COMMAND_STATE"
    ) && !matches!(
        event.string.as_deref(),
        Some("photoZoomIn" | "photoZoomOut")
    )
}
