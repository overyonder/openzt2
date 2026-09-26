//! Blue Fang source-document parsing for live asset lowering.
//!
//! This parser stops at a faithful ordered source tree. Source-family consumers
//! lower that transient tree into their owned asset type and then discard it.

use std::{ops::Range, path::Path};

use smol_str::SmolStr;

use super::{
    blue_fang_source_document_format::BlueFangSourceDocumentFormat,
    blue_fang_source_document_parsing_error::BlueFangSourceDocumentParsingError,
    ordered_source_document_types::{
        OrderedSourceDocument, OrderedSourceDocumentAttribute, OrderedSourceDocumentChild,
        OrderedSourceDocumentChildren, OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
        OrderedSourceDocumentText,
    },
    path::AssetPath,
};

const BFXML_PREFIX_NAMESPACE_BASE: &str = "urn:openzt2:bfxml-prefix:";

/// Parses one XML/BFXML-family source document into its ordered transient tree.
pub(crate) fn parse_blue_fang_source_document(
    source_document_path: AssetPath,
    source_document_bytes: &[u8],
) -> Result<OrderedSourceDocument, BlueFangSourceDocumentParsingError> {
    let source_document_format = BlueFangSourceDocumentFormat::from_source_document_path(
        &source_document_path,
    )
    .ok_or_else(|| BlueFangSourceDocumentParsingError::UnsupportedPath {
        path: source_document_path.clone(),
    })?;
    let (_, decoded_source_document_text) =
        z2f::text::decoding::decode_blue_fang_source_text_from_utf8_or_utf16_bytes(
            source_document_bytes,
        )
        .map_err(|error| BlueFangSourceDocumentParsingError::InvalidText {
            path: source_document_path.clone(),
            message: error.to_string(),
        })?;
    let repaired_source_document_text =
        z2f::text::xml_like_document_syntax::repair_observed_blue_fang_xml_like_document_syntax(
            Path::new(source_document_path.as_str()),
            &decoded_source_document_text,
        );
    let parsed_source_document = roxmltree::Document::parse(&repaired_source_document_text)
        .map_err(|error| BlueFangSourceDocumentParsingError::InvalidMarkup {
            path: source_document_path.clone(),
            message: error.to_string(),
        })?;

    Ok(OrderedSourceDocument {
        path: source_document_path,
        format: source_document_format,
        root: copy_blue_fang_source_document_node(parsed_source_document.root_element()),
    })
}

fn copy_blue_fang_source_document_node(
    source_document_node: roxmltree::Node<'_, '_>,
) -> OrderedSourceDocumentNode {
    OrderedSourceDocumentNode {
        name: recover_blue_fang_xml_source_element_name(source_document_node),
        attributes: copy_blue_fang_source_document_attributes(source_document_node),
        children: copy_blue_fang_source_document_children(source_document_node),
        span: ordered_source_document_span_from_repaired_utf8_range(source_document_node.range()),
    }
}

fn copy_blue_fang_source_document_attributes(
    source_document_node: roxmltree::Node<'_, '_>,
) -> Vec<OrderedSourceDocumentAttribute> {
    source_document_node
        .attributes()
        .map(|source_document_attribute| OrderedSourceDocumentAttribute {
            name: SmolStr::new(source_document_attribute.name()),
            value: SmolStr::new(source_document_attribute.value()),
        })
        .collect()
}

fn copy_blue_fang_source_document_children(
    source_document_node: roxmltree::Node<'_, '_>,
) -> OrderedSourceDocumentChildren {
    OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        source_document_node
            .children()
            .filter_map(|source_document_child| {
                if source_document_child.is_element() {
                    Some(OrderedSourceDocumentChild::Element(Box::new(
                        copy_blue_fang_source_document_node(source_document_child),
                    )))
                } else if source_document_child.is_text() {
                    source_document_child
                        .text()
                        .and_then(normalize_blue_fang_source_document_text_child)
                } else if source_document_child.is_comment() {
                    Some(OrderedSourceDocumentChild::Comment)
                } else if source_document_child.is_pi() {
                    Some(OrderedSourceDocumentChild::ProcessingInstruction)
                } else {
                    None
                }
            })
            .collect(),
    )
}

fn normalize_blue_fang_source_document_text_child(
    source_document_text: &str,
) -> Option<OrderedSourceDocumentChild> {
    if source_document_text.trim().is_empty() {
        return None;
    }

    let normalized_source_document_text =
        if source_document_text.contains('\r') || source_document_text.contains('\n') {
            source_document_text.trim()
        } else {
            source_document_text
        };

    Some(OrderedSourceDocumentChild::Text(
        OrderedSourceDocumentText {
            value: SmolStr::new(normalized_source_document_text),
        },
    ))
}

fn ordered_source_document_span_from_repaired_utf8_range(
    repaired_utf8_range: Range<usize>,
) -> OrderedSourceDocumentSpan {
    OrderedSourceDocumentSpan {
        start: repaired_utf8_range.start,
        end: repaired_utf8_range.end,
    }
}

fn recover_blue_fang_xml_source_element_name(
    source_document_node: roxmltree::Node<'_, '_>,
) -> SmolStr {
    let source_document_tag_name = source_document_node.tag_name();
    source_document_tag_name
        .namespace()
        .and_then(|namespace| namespace.strip_prefix(BFXML_PREFIX_NAMESPACE_BASE))
        .map_or_else(
            || SmolStr::new(source_document_tag_name.name()),
            |prefix| SmolStr::new(format!("{prefix}:{}", source_document_tag_name.name())),
        )
}
