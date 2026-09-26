//! Authored biome-panel extraction and in-game shell composition.

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocument,
    ui::model::{SourceUiEvent, SourceUiNode},
};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    source_ui_diagnostic_to_document_gap_conversion::convert_source_ui_diagnostic_to_document_gap,
    ui_source_document_parsing_and_normalization::parse_and_normalize_blue_fang_ui_source_document,
};

pub(super) fn parse_embedded_biome_panel_from_source_document(
    document: &OrderedSourceDocument,
) -> Result<Option<SourceUiNode>, UiSourceDocumentGap> {
    if !document.root.name.eq_ignore_ascii_case("BFGBiome") {
        return Ok(None);
    }
    let Some(biome) = document.root.attribute("name") else {
        return Err(UiSourceDocumentGap::at_root(
            document,
            UiSourceDocumentFamily::Ui,
            UiSourceDocumentGapKind::UnsupportedVocabulary,
            "BFGBiome has no canonical name for its authored terrain panel",
        ));
    };
    let panels = document
        .root
        .element_children()
        .filter(|node| node.name.eq_ignore_ascii_case("panel"))
        .collect::<Vec<_>>();
    let Some(panel) = (panels.len() == 1).then(|| panels[0]) else {
        return Ok(None);
    };
    let roots = panel.element_children().collect::<Vec<_>>();
    let Some(root) = (roots.len() == 1).then(|| roots[0]) else {
        return Err(UiSourceDocumentGap::at_node(
            document,
            panel,
            UiSourceDocumentFamily::Ui,
            UiSourceDocumentGapKind::UnsupportedVocabulary,
            "BFGBiome panel must contain exactly one authored UI root",
        ));
    };
    let subtree = OrderedSourceDocument {
        path: document.path.clone(),
        format: document.format,
        root: root.clone(),
    };
    let mut source = parse_and_normalize_blue_fang_ui_source_document(&subtree)?;
    if let Some(diagnostic) = source.diagnostics.first() {
        return Err(convert_source_ui_diagnostic_to_document_gap(
            &source, diagnostic,
        ));
    }
    source.root.name = Some(format!("openzt2 biome panel:{biome}"));
    source.root.state.visible = false;
    attach_authored_biome_identity_to_ui_source_events(&mut source.root, biome);
    Ok(Some(source.root))
}

fn attach_authored_biome_identity_to_ui_source_events(node: &mut SourceUiNode, biome: &str) {
    for block in &mut node.events {
        if block.events.iter().any(|event| {
            event.message == "ZT_SETMODE"
                && matches!(
                    event.string.as_deref(),
                    Some(
                        "deepwater"
                            | "shallowwater"
                            | "ground"
                            | "mix"
                            | "ground-cover"
                            | "foliage-mix"
                    )
                )
        }) {
            block.events.insert(
                0,
                SourceUiEvent {
                    message: "ZT_SET_BIOME".into(),
                    data: Some("BFString".into()),
                    string: Some(biome.into()),
                    value: None,
                    key: None,
                    val: None,
                    event_type: None,
                    target_child: None,
                    color: None,
                    rect: [None, None, None, None],
                    xml_object: None,
                    child: None,
                    payload: Vec::new(),
                    unknown_attributes: Vec::new(),
                },
            );
        }
    }
    node.children.iter_mut().for_each(|child| {
        attach_authored_biome_identity_to_ui_source_events(child, biome);
    });
}

pub(super) fn attach_embedded_biome_panels_to_authored_shell_composition_point(
    node: &mut SourceUiNode,
    panels: &mut Vec<SourceUiNode>,
) -> bool {
    if node
        .name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case("Biome Panel"))
    {
        node.children.append(panels);
        return true;
    }
    node.children.iter_mut().any(|child| {
        attach_embedded_biome_panels_to_authored_shell_composition_point(child, panels)
    })
}
