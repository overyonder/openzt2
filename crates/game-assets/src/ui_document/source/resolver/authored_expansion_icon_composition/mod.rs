//! Expansion-owned UI fragments composed into the main menu's icon lists.

use crate::source_document::{
    blue_fang_source_document_format::BlueFangSourceDocumentFormat,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
    path::AssetPath,
    ui::{
        model::{SourceUiAlignment, SourceUiMetric, SourceUiNode, SourceUiRegion},
        parser::{source_scalar_and_attribute_reading::parse_source_bool, SourceUiDocument},
    },
};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    source_ui_diagnostic_to_document_gap_conversion::convert_source_ui_diagnostic_to_document_gap,
    ui_source_document_parsing_and_normalization::parse_and_normalize_blue_fang_ui_source_document,
};

pub(super) fn parse_expansion_icon_from_source_document(
    document: &OrderedSourceDocument,
    icon_element: &str,
) -> Result<Option<(i32, SourceUiNode)>, UiSourceDocumentGap> {
    if !document.root.name.eq_ignore_ascii_case("ZTExpansionInfo")
        || document
            .root
            .attribute("installed")
            .filter(|installed| !installed.is_empty())
            .is_some_and(|installed| !parse_source_bool(installed).unwrap_or(false))
    {
        return Ok(None);
    }
    let Some(icon) = document
        .root
        .element_children()
        .find(|child| child.name.eq_ignore_ascii_case(icon_element))
    else {
        return Ok(None);
    };
    let mut children = icon.element_children();
    let Some(root) = children.next() else {
        return Ok(None);
    };
    if children.next().is_some() {
        return Err(UiSourceDocumentGap::at_node(
            document,
            icon,
            UiSourceDocumentFamily::Ui,
            UiSourceDocumentGapKind::UnsupportedVocabulary,
            "expansion icon must contain exactly one authored UI root",
        ));
    }
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
    source.root.name = Some(format!("{}:{icon_element}", document.path.key()));
    let expansion_index = document
        .root
        .attribute("index")
        .and_then(parse_blue_fang_source_numeric_lexeme::<i32>)
        .unwrap_or(0);
    Ok(Some((expansion_index, source.root)))
}

pub(super) fn attach_expansion_icons_to_authored_menu_list(
    node: &mut SourceUiNode,
    list_name: &str,
    icons: &mut Vec<(i32, SourceUiNode)>,
) -> Result<(), UiSourceDocumentGap> {
    if node.name.as_deref() == Some(list_name) {
        node.children.clear();
        let mut total_icon_width = 0_i32;
        icons.sort_by_key(|(expansion_index, _)| *expansion_index);
        for (index, (_, icon)) in icons.drain(..).enumerate() {
            let width = match icon.region.as_ref().map(|region| &region.width) {
                None => 0,
                Some(SourceUiMetric::Number(width)) if width.is_finite() => *width as i32,
                _ => {
                    return Err(UiSourceDocumentGap {
                        family: UiSourceDocumentFamily::Ui,
                        kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                        virtual_path: "ui/layout/mainmenu.xml".into(),
                        span: icon.span,
                        message: "expansion icon requires a resolved numeric width".into(),
                    })
                }
            };
            total_icon_width = total_icon_width.wrapping_add(width);
            node.children.push(icon);
            if index == 1 {
                node.children.push(create_expansion_icon_center_spacer(
                    list_name,
                    644_i32.wrapping_sub(total_icon_width),
                ));
            }
        }
        return Ok(());
    }
    for child in &mut node.children {
        attach_expansion_icons_to_authored_menu_list(child, list_name, icons)?;
    }
    Ok(())
}

fn create_expansion_icon_center_spacer(list_name: &str, width: i32) -> SourceUiNode {
    let source = OrderedSourceDocument {
        path: AssetPath::new("ui/layout/mainmenu.xml"),
        format: BlueFangSourceDocumentFormat::Xml,
        root: OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
            "UIStatic".into(),
            Vec::new(),
            Default::default(),
            Default::default(),
        ),
    };
    let mut spacer = SourceUiDocument::parse(&source).root;
    spacer.name = Some(format!("{list_name}:center spacer"));
    spacer.region = Some(SourceUiRegion {
        x: SourceUiMetric::Number(0.0),
        y: SourceUiMetric::Number(0.0),
        width: SourceUiMetric::Number(width as f32),
        height: SourceUiMetric::Number(1.0),
        x_align: SourceUiAlignment::Min,
        y_align: SourceUiAlignment::Min,
        width_align: SourceUiAlignment::Length,
        height_align: SourceUiAlignment::Length,
        unknown_attributes: Vec::new(),
    });
    spacer
}
